//! Resumable downloads + throughput metering for the OS updater.
//!
//! The update bundle (~640MB) streams straight onto the inactive
//! slot's partitions, so a dropped connection used to throw away the
//! whole download. [`ResumableReader`] hides interruptions from the
//! consumer (tar parser -> zstd -> sha256): on a read error, a silent
//! stall or an early EOF it reopens the remote object at the current
//! offset (HTTP Range in update.rs) and carries on, so the bytes the
//! caller sees are exactly `[0, total)` of the object.
//!
//! Platform-independent (no HTTP client here - the opener is injected)
//! so it is unit-tested everywhere; only the Linux updater uses it.

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, ReadBuf};
use tokio::time::Sleep;

// ---------------------------------------------------------------------------
// Throughput
// ---------------------------------------------------------------------------

/// Throughput over a sliding window of `(time, cumulative bytes)`
/// samples. Feed it the running total (not deltas) so a reconnect
/// that re-reads nothing can't double count.
pub struct RateMeter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl Default for RateMeter {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

impl RateMeter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Record the cumulative byte count at `now`.
    pub fn sample(&mut self, now: Instant, total: u64) {
        if self.samples.back().is_some_and(|&(_, last)| total < last) {
            // Counter went backwards (a fresh transfer): start over.
            self.samples.clear();
        }
        self.samples.push_back((now, total));
        // Keep one sample at or before the window start as the
        // baseline, so the rate always spans (about) the full window.
        while self.samples.len() > 2 && self.samples[1].0 + self.window <= now {
            self.samples.pop_front();
        }
    }

    /// Bytes per second over the window ending at `now`. 0 until two
    /// samples exist, and decays to 0 once no bytes arrive for a whole
    /// window.
    pub fn rate(&self, now: Instant) -> u64 {
        let (Some(&(t0, b0)), Some(&(t1, b1))) = (self.samples.front(), self.samples.back()) else {
            return 0;
        };
        if t1 <= t0 {
            return 0;
        }
        // Bytes that arrived within the window: everything after the
        // newest sample at or before the window start.
        let start = now.checked_sub(self.window).unwrap_or(t0);
        let (base_t, base_b) = self
            .samples
            .iter()
            .rev()
            .find(|&&(t, _)| t <= start)
            .copied()
            .unwrap_or((t0, b0));
        let span = now.saturating_duration_since(base_t).max(t1 - base_t);
        if span.is_zero() {
            return 0;
        }
        ((b1 - base_b) as f64 / span.as_secs_f64()) as u64
    }
}

// ---------------------------------------------------------------------------
// Resumable reader
// ---------------------------------------------------------------------------

/// Why opening (or reopening) the remote object failed.
#[derive(Debug)]
pub enum OpenError {
    /// Transient (network, 5xx, ...): back off and try again.
    Retry(io::Error),
    /// Permanent (the object changed, the server refuses ranges, 404):
    /// give up now.
    Fatal(io::Error),
}

pub type BoxReader = Box<dyn AsyncRead + Send + Unpin>;
pub type OpenFuture = Pin<Box<dyn Future<Output = Result<BoxReader, OpenError>> + Send>>;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// No bytes for this long (while connecting or reading) counts as
    /// a failure. Catches silent stalls long before the kernel's
    /// ~15 minute TCP retransmission timeout.
    pub idle_timeout: Duration,
    /// Consecutive failures (without a byte delivered in between)
    /// before giving up.
    pub max_attempts: u32,
    /// Backoff before reopen attempt N is `base * 2^(N-1)`, capped.
    pub backoff_base: Duration,
    pub backoff_max: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(30),
            max_attempts: 8,
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    fn backoff(&self, failures: u32) -> Duration {
        let exp = failures.saturating_sub(1).min(16);
        self.backoff_base
            .saturating_mul(1u32 << exp)
            .min(self.backoff_max)
    }
}

enum State {
    /// Nothing open yet (or about to reopen after backoff).
    Start,
    Opening(OpenFuture),
    Reading(BoxReader),
    Backoff(Pin<Box<Sleep>>),
    Done,
    Failed,
}

/// An [`AsyncRead`] over bytes `[0, total)` of a remote object that
/// transparently reopens at the current offset (via `opener(offset)`)
/// after errors, stalls and premature EOFs.
pub struct ResumableReader<F>
where
    F: FnMut(u64) -> OpenFuture + Send + Unpin,
{
    total: u64,
    pos: u64,
    opener: F,
    policy: RetryPolicy,
    delivered: Arc<AtomicU64>,
    state: State,
    idle: Pin<Box<Sleep>>,
    /// The caller is blocked in a read. The idle timeout only runs
    /// then: time the consumer spends elsewhere (e.g. syncing a
    /// partition to the SD card) is not a network stall.
    waiting: bool,
    failures: u32,
}

impl<F> ResumableReader<F>
where
    F: FnMut(u64) -> OpenFuture + Send + Unpin,
{
    /// `delivered` is bumped by every byte handed to the caller (the
    /// progress/speed source); it should start at 0.
    pub fn new(total: u64, opener: F, policy: RetryPolicy, delivered: Arc<AtomicU64>) -> Self {
        let idle = Box::pin(tokio::time::sleep(policy.idle_timeout));
        Self {
            total,
            pos: 0,
            opener,
            policy,
            delivered,
            state: State::Start,
            idle,
            waiting: false,
            failures: 0,
        }
    }

    fn reset_idle(&mut self) {
        let deadline = tokio::time::Instant::now() + self.policy.idle_timeout;
        self.idle.as_mut().reset(deadline);
    }

    /// Record a failure: schedule a reopen, or give up once the
    /// consecutive-failure budget is spent.
    fn fail(&mut self, err: io::Error) -> io::Result<()> {
        self.failures += 1;
        let cause = error_chain(&err);
        if self.failures >= self.policy.max_attempts {
            self.state = State::Failed;
            return Err(io::Error::new(
                err.kind(),
                format!("download failed after {} attempts: {cause}", self.failures),
            ));
        }
        tracing::warn!(
            offset = self.pos,
            attempt = self.failures,
            err = %cause,
            "update download interrupted; resuming"
        );
        let delay = self.policy.backoff(self.failures);
        self.state = State::Backoff(Box::pin(tokio::time::sleep(delay)));
        Ok(())
    }

    fn stalled(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "no data received for {}s",
                self.policy.idle_timeout.as_secs()
            ),
        )
    }
}

impl<F> AsyncRead for ResumableReader<F>
where
    F: FnMut(u64) -> OpenFuture + Send + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if !this.waiting {
            this.waiting = true;
            this.reset_idle();
        }
        let res = this.poll_fill(cx, buf);
        if res.is_ready() {
            this.waiting = false;
        }
        res
    }
}

impl<F> ResumableReader<F>
where
    F: FnMut(u64) -> OpenFuture + Send + Unpin,
{
    fn poll_fill(&mut self, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self;
        loop {
            match &mut this.state {
                State::Done => return Poll::Ready(Ok(())),
                State::Failed => {
                    return Poll::Ready(Err(io::Error::other("download already failed")))
                }
                State::Start => {
                    if this.pos >= this.total {
                        this.state = State::Done;
                        continue;
                    }
                    this.state = State::Opening((this.opener)(this.pos));
                    this.reset_idle();
                }
                State::Backoff(sleep) => {
                    if sleep.as_mut().poll(cx).is_pending() {
                        return Poll::Pending;
                    }
                    this.state = State::Start;
                }
                State::Opening(fut) => match fut.as_mut().poll(cx) {
                    Poll::Ready(Ok(reader)) => {
                        this.state = State::Reading(reader);
                        this.reset_idle();
                    }
                    Poll::Ready(Err(OpenError::Fatal(err))) => {
                        this.state = State::Failed;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Ready(Err(OpenError::Retry(err))) => {
                        if let Err(err) = this.fail(err) {
                            return Poll::Ready(Err(err));
                        }
                    }
                    Poll::Pending => {
                        if this.idle.as_mut().poll(cx).is_pending() {
                            return Poll::Pending;
                        }
                        let err = this.stalled();
                        if let Err(err) = this.fail(err) {
                            return Poll::Ready(Err(err));
                        }
                    }
                },
                State::Reading(reader) => {
                    if this.pos >= this.total {
                        this.state = State::Done;
                        continue;
                    }
                    let before = buf.filled().len();
                    match Pin::new(reader).poll_read(cx, buf) {
                        Poll::Ready(Ok(())) => {
                            let n = (buf.filled().len() - before) as u64;
                            if n == 0 {
                                let err = io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    format!(
                                        "connection closed early at byte {} of {}",
                                        this.pos, this.total
                                    ),
                                );
                                if let Err(err) = this.fail(err) {
                                    return Poll::Ready(Err(err));
                                }
                                continue;
                            }
                            if this.pos + n > this.total {
                                buf.set_filled(before);
                                this.state = State::Failed;
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!(
                                        "download is larger than the expected {} bytes",
                                        this.total
                                    ),
                                )));
                            }
                            this.pos += n;
                            this.delivered.fetch_add(n, Ordering::Relaxed);
                            this.failures = 0;
                            this.reset_idle();
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Ready(Err(err)) => {
                            buf.set_filled(before);
                            if let Err(err) = this.fail(err) {
                                return Poll::Ready(Err(err));
                            }
                        }
                        Poll::Pending => {
                            if this.idle.as_mut().poll(cx).is_pending() {
                                return Poll::Pending;
                            }
                            let err = this.stalled();
                            if let Err(err) = this.fail(err) {
                                return Poll::Ready(Err(err));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// "outer: cause: root cause" - HTTP client errors hide the useful
/// part (timeout? reset? DNS?) in their source chain, and these
/// strings surface verbatim in the settings UIs.
pub fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = err.to_string();
    let mut cur = err.source();
    while let Some(e) = cur {
        let s = e.to_string();
        if !out.ends_with(&s) {
            out.push_str(": ");
            out.push_str(&s);
        }
        cur = e.source();
    }
    out
}

/// Parse an HTTP `Content-Range: bytes START-END/TOTAL` value into
/// `(start, end_inclusive, total)`; `total` is None for `*`.
pub fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    Some((start, end, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::AsyncReadExt;

    // -- RateMeter ----------------------------------------------------------

    #[test]
    fn rate_constant() {
        let t0 = Instant::now();
        let mut m = RateMeter::default();
        for i in 0..=10u64 {
            m.sample(t0 + Duration::from_secs(i), i * 1_000_000);
        }
        assert_eq!(m.rate(t0 + Duration::from_secs(10)), 1_000_000);
    }

    #[test]
    fn rate_startup() {
        let t0 = Instant::now();
        let mut m = RateMeter::default();
        assert_eq!(m.rate(t0), 0);
        m.sample(t0, 500);
        assert_eq!(m.rate(t0), 0);
        assert_eq!(m.rate(t0 + Duration::from_secs(1)), 0);
        m.sample(t0 + Duration::from_secs(1), 2_500);
        assert_eq!(m.rate(t0 + Duration::from_secs(1)), 2_000);
    }

    #[test]
    fn rate_stall_decays_to_zero() {
        let t0 = Instant::now();
        let mut m = RateMeter::default();
        for i in 0..=5u64 {
            m.sample(t0 + Duration::from_secs(i), i * 1000);
        }
        assert_eq!(m.rate(t0 + Duration::from_secs(5)), 1000);
        // Stalled: the total stops moving.
        for i in 6..=8u64 {
            m.sample(t0 + Duration::from_secs(i), 5000);
        }
        let mid = m.rate(t0 + Duration::from_secs(8));
        assert!(mid > 0 && mid < 1000, "{mid}");
        for i in 9..=11u64 {
            m.sample(t0 + Duration::from_secs(i), 5000);
        }
        assert_eq!(m.rate(t0 + Duration::from_secs(11)), 0);
        // Also decays without fresh samples.
        let mut m = RateMeter::default();
        m.sample(t0, 0);
        m.sample(t0 + Duration::from_secs(1), 1000);
        assert_eq!(m.rate(t0 + Duration::from_secs(20)), 0);
    }

    #[test]
    fn rate_resume_does_not_double_count() {
        // The counter is "bytes delivered", which a reconnect at the
        // current offset leaves untouched: a reconnect pause just
        // shows as a dip, never a spike.
        let t0 = Instant::now();
        let mut m = RateMeter::default();
        let totals = [0, 1000, 2000, 2000, 2000, 3000, 4000, 5000, 6000, 7000];
        for (i, &b) in totals.iter().enumerate() {
            m.sample(t0 + Duration::from_secs(i as u64), b);
            assert!(m.rate(t0 + Duration::from_secs(i as u64)) <= 1000);
        }
        assert_eq!(m.rate(t0 + Duration::from_secs(9)), 1000);
        // A counter reset (new transfer) starts over instead of
        // computing a bogus value.
        m.sample(t0 + Duration::from_secs(10), 0);
        assert_eq!(m.rate(t0 + Duration::from_secs(10)), 0);
    }

    // -- ResumableReader ----------------------------------------------------

    fn data(len: usize) -> Arc<Vec<u8>> {
        Arc::new((0..len).map(|i| (i * 31 % 251) as u8).collect())
    }

    /// How one opened connection behaves after it starts at `offset`.
    #[derive(Clone, Copy)]
    enum Conn {
        /// Serve to the end.
        Full,
        /// Serve `n` bytes, then a read error.
        ErrorAfter(usize),
        /// Serve `n` bytes, then EOF.
        EofAfter(usize),
        /// Serve `n` bytes, then hang forever.
        StallAfter(usize),
        /// Serve to the end, but report Pending once after `n` bytes
        /// (data not yet arrived), waking immediately.
        PendOnceAfter(usize),
    }

    struct FakeReader {
        data: Arc<Vec<u8>>,
        pos: usize,
        left: usize,
        conn: Conn,
        served: usize,
        pended: bool,
    }

    impl AsyncRead for FakeReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if let Conn::PendOnceAfter(n) = self.conn {
                if self.served >= n && !self.pended {
                    self.pended = true;
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
            if self.left == 0 {
                return match self.conn {
                    Conn::Full | Conn::EofAfter(_) | Conn::PendOnceAfter(_) => Poll::Ready(Ok(())),
                    Conn::ErrorAfter(_) => Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "connection reset",
                    ))),
                    Conn::StallAfter(_) => Poll::Pending,
                };
            }
            // Small chunks so failures land mid-stream.
            let mut n = self.left.min(buf.remaining()).min(7);
            if let Conn::PendOnceAfter(after) = self.conn {
                if self.served < after {
                    n = n.min(after - self.served);
                }
            }
            let pos = self.pos;
            buf.put_slice(&self.data[pos..pos + n]);
            self.pos += n;
            self.left -= n;
            self.served += n;
            Poll::Ready(Ok(()))
        }
    }

    enum Plan {
        Conn(Conn),
        Retry,
        Fatal,
    }

    /// Opener that follows `plans` in order (then serves `Full`),
    /// recording the requested offsets.
    fn opener(
        data: Arc<Vec<u8>>,
        plans: Vec<Plan>,
        offsets: Arc<Mutex<Vec<u64>>>,
    ) -> impl FnMut(u64) -> OpenFuture + Send + Unpin {
        let plans = Arc::new(Mutex::new(VecDeque::from(plans)));
        move |offset| {
            offsets.lock().unwrap().push(offset);
            let plan = plans
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Plan::Conn(Conn::Full));
            let data = data.clone();
            Box::pin(async move {
                let conn = match plan {
                    Plan::Retry => {
                        return Err(OpenError::Retry(io::Error::other(
                            "503 Service Unavailable",
                        )))
                    }
                    Plan::Fatal => return Err(OpenError::Fatal(io::Error::other("404 Not Found"))),
                    Plan::Conn(c) => c,
                };
                let pos = offset as usize;
                let rest = data.len() - pos;
                let left = match conn {
                    Conn::Full | Conn::PendOnceAfter(_) => rest,
                    Conn::ErrorAfter(n) | Conn::EofAfter(n) | Conn::StallAfter(n) => n.min(rest),
                };
                Ok(Box::new(FakeReader {
                    data,
                    pos,
                    left,
                    conn,
                    served: 0,
                    pended: false,
                }) as BoxReader)
            })
        }
    }

    async fn run(
        len: usize,
        plans: Vec<Plan>,
        policy: RetryPolicy,
    ) -> (io::Result<Vec<u8>>, Vec<u64>, u64, Arc<Vec<u8>>) {
        let d = data(len);
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(AtomicU64::new(0));
        let mut r = ResumableReader::new(
            len as u64,
            opener(d.clone(), plans, offsets.clone()),
            policy,
            delivered.clone(),
        );
        let mut out = Vec::new();
        let res = r.read_to_end(&mut out).await.map(|_| out);
        let offs = offsets.lock().unwrap().clone();
        (res, offs, delivered.load(Ordering::Relaxed), d)
    }

    #[tokio::test]
    async fn uninterrupted() {
        tokio::time::pause();
        let (res, offs, delivered, d) = run(1000, vec![], RetryPolicy::default()).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0]);
        assert_eq!(delivered, 1000);
    }

    #[tokio::test]
    async fn resumes_after_read_error() {
        tokio::time::pause();
        let plans = vec![
            Plan::Conn(Conn::ErrorAfter(300)),
            Plan::Conn(Conn::ErrorAfter(250)),
        ];
        let (res, offs, delivered, d) = run(1000, plans, RetryPolicy::default()).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0, 300, 550]);
        assert_eq!(delivered, 1000);
    }

    #[tokio::test]
    async fn resumes_after_stall() {
        tokio::time::pause();
        let start = tokio::time::Instant::now();
        let plans = vec![Plan::Conn(Conn::StallAfter(400))];
        let (res, offs, delivered, d) = run(1000, plans, RetryPolicy::default()).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0, 400]);
        assert_eq!(delivered, 1000);
        // idle timeout (30s) + first backoff (1s), on the paused clock.
        assert!(start.elapsed() >= Duration::from_secs(31));
    }

    #[tokio::test]
    async fn slow_consumer_is_not_a_stall() {
        // The updater stops reading while it syncs a partition to the
        // SD card; time spent away from read() must not count toward
        // the idle timeout once it comes back to a momentarily empty
        // connection.
        tokio::time::pause();
        let d = data(1000);
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(AtomicU64::new(0));
        let mut r = ResumableReader::new(
            1000,
            opener(
                d.clone(),
                vec![Plan::Conn(Conn::PendOnceAfter(400))],
                offsets.clone(),
            ),
            RetryPolicy::default(),
            delivered,
        );
        let mut out = vec![0u8; 400];
        r.read_exact(&mut out).await.unwrap();
        tokio::time::sleep(Duration::from_secs(120)).await;
        r.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, *d);
        assert_eq!(*offsets.lock().unwrap(), vec![0]);
    }

    #[tokio::test]
    async fn resumes_after_premature_eof() {
        tokio::time::pause();
        let plans = vec![Plan::Conn(Conn::EofAfter(999))];
        let (res, offs, _, d) = run(1000, plans, RetryPolicy::default()).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0, 999]);
    }

    #[tokio::test]
    async fn retries_open_errors() {
        tokio::time::pause();
        let plans = vec![Plan::Retry, Plan::Conn(Conn::ErrorAfter(10)), Plan::Retry];
        let (res, offs, _, d) = run(100, plans, RetryPolicy::default()).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0, 0, 10, 10]);
    }

    #[tokio::test]
    async fn fatal_open_stops() {
        tokio::time::pause();
        let plans = vec![Plan::Conn(Conn::ErrorAfter(100)), Plan::Fatal];
        let (res, offs, delivered, _) = run(1000, plans, RetryPolicy::default()).await;
        let err = res.unwrap_err();
        assert!(err.to_string().contains("404"), "{err}");
        assert_eq!(offs, vec![0, 100]);
        assert_eq!(delivered, 100);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        tokio::time::pause();
        let policy = RetryPolicy {
            max_attempts: 3,
            ..Default::default()
        };
        let plans = (0..10).map(|_| Plan::Retry).collect();
        let (res, offs, _, _) = run(1000, plans, policy).await;
        let err = res.unwrap_err();
        assert_eq!(
            err.to_string(),
            "download failed after 3 attempts: 503 Service Unavailable"
        );
        assert_eq!(offs.len(), 3);
    }

    #[tokio::test]
    async fn progress_resets_failure_budget() {
        tokio::time::pause();
        let policy = RetryPolicy {
            max_attempts: 2,
            ..Default::default()
        };
        // Each connection makes progress before failing, so 5 drops
        // never exhaust a budget of 2 consecutive failures.
        let plans = (0..5).map(|_| Plan::Conn(Conn::ErrorAfter(100))).collect();
        let (res, offs, delivered, d) = run(1000, plans, policy).await;
        assert_eq!(res.unwrap(), *d);
        assert_eq!(offs, vec![0, 100, 200, 300, 400, 500]);
        assert_eq!(delivered, 1000);
    }

    #[tokio::test]
    async fn oversized_body_is_an_error() {
        tokio::time::pause();
        let d = data(1000);
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(AtomicU64::new(0));
        // Claims 500 bytes but the server sends 1000.
        let mut r = ResumableReader::new(
            500,
            opener(d, vec![], offsets),
            RetryPolicy::default(),
            delivered.clone(),
        );
        let mut out = Vec::new();
        let err = r.read_to_end(&mut out).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(delivered.load(Ordering::Relaxed) <= 500);
    }

    #[test]
    fn backoff_schedule() {
        let p = RetryPolicy::default();
        let secs: Vec<u64> = (1..=8).map(|n| p.backoff(n).as_secs()).collect();
        assert_eq!(secs, vec![1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn content_range() {
        assert_eq!(
            parse_content_range("bytes 100-999/1000"),
            Some((100, 999, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 0-9/*"), Some((0, 9, None)));
        assert_eq!(parse_content_range("bytes */1000"), None);
        assert_eq!(parse_content_range("bytes 9-0/10"), None);
        assert_eq!(parse_content_range("items 0-9/10"), None);
    }

    #[test]
    fn chain_formatting() {
        #[derive(Debug)]
        struct Outer(io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("error decoding response body")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let e = io::Error::other(Outer(io::Error::other("Connection timed out")));
        assert_eq!(
            error_chain(&e),
            "error decoding response body: Connection timed out"
        );
    }
}
