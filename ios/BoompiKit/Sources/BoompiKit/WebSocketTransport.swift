// WebSocket transport to boompid's /ws endpoint - the network
// counterpart of the BLE GATT bridge. Same JSON protocol, no chunk
// framing (WebSocket messages arrive whole), and the server greets
// with hello + full state on connect, so the client state machine is
// identical from the first byte.
//
// Bonjour endpoints can't be handed to URLSessionWebSocketTask (it
// wants a URL, and Network.framework's own WebSocket can't set the
// /ws path on a service endpoint), so connection is two-step: a
// throwaway TCP probe lets Network.framework do the SRV/A resolution
// and Happy Eyeballs, then the winning host:port becomes a ws:// URL.

import Foundation
import Network

/// ws:// URL formatting for resolved endpoints. Nonisolated + pure so
/// BoompiKitChecks can exercise the host quoting (IPv6 literals need
/// brackets, and link-local scopes a %25 escape).
public enum WSURL {
    public static func string(host: NWEndpoint.Host, port: UInt16) -> String {
        let quoted: String
        switch host {
        case .name(let name, _):
            quoted = name
        case .ipv4(let addr):
            quoted = "\(addr)"
        case .ipv6(let addr):
            quoted = "[" + "\(addr)".replacingOccurrences(of: "%", with: "%25") + "]"
        @unknown default:
            quoted = "\(host)"
        }
        return "ws://\(quoted):\(port)/ws"
    }
}

@MainActor
final class WebSocketTransport: NSObject {
    /// Handshake complete; the greeting (hello + state) is on its way.
    var onOpen: (() -> Void)?
    /// One complete JSON ServerMessage (text frame). Binary frames
    /// (visualizer bars) are dropped - the phone UI doesn't draw them.
    var onMessage: ((Data) -> Void)?
    /// Link is gone (fires at most once; never after `close()`).
    var onClose: ((String) -> Void)?

    private var probe: NWConnection?
    private var session: URLSession?
    private var task: URLSessionWebSocketTask?
    private var pinger: Task<Void, Never>?
    private var closed = false

    /// Liveness pings: Wi-Fi links die silently (box powered off,
    /// phone roamed); a failed pong is the only prompt signal.
    private static let pingIntervalSecs: UInt64 = 15

    func connect(to endpoint: NWEndpoint) {
        let probe = NWConnection(to: endpoint, using: .tcp)
        self.probe = probe
        probe.stateUpdateHandler = { [weak self] state in
            // Started on the main queue: assumeIsolated is sound.
            MainActor.assumeIsolated {
                guard let self, !self.closed, self.probe === probe else { return }
                switch state {
                case .ready:
                    let remote = probe.currentPath?.remoteEndpoint
                    probe.cancel()
                    self.probe = nil
                    guard case .hostPort(let host, let port) = remote,
                          let url = URL(string: WSURL.string(host: host, port: port.rawValue))
                    else {
                        self.finish("could not resolve the speaker's address")
                        return
                    }
                    self.open(url)
                case .failed(let err):
                    probe.cancel()
                    self.probe = nil
                    self.finish(err.localizedDescription)
                case .waiting(let err):
                    // No route (wrong network, box gone): fail fast
                    // instead of letting the connect timeout fire.
                    probe.cancel()
                    self.probe = nil
                    self.finish(err.localizedDescription)
                default:
                    break
                }
            }
        }
        probe.start(queue: .main)
    }

    func send(_ data: Data) {
        guard let task, let text = String(data: data, encoding: .utf8) else { return }
        task.send(.string(text)) { [weak self] error in
            guard error != nil else { return }
            Task { @MainActor [weak self] in
                self?.finish("send failed")
            }
        }
    }

    /// Tear down silently: `onClose` will not fire.
    func close() {
        closed = true
        pinger?.cancel()
        pinger = nil
        probe?.cancel()
        probe = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        session?.invalidateAndCancel()
        session = nil
    }

    private func open(_ url: URL) {
        let session = URLSession(
            configuration: .default,
            delegate: self,
            delegateQueue: .main
        )
        self.session = session
        let task = session.webSocketTask(with: url)
        self.task = task
        task.resume()
        receiveLoop()
        pinger = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: Self.pingIntervalSecs * 1_000_000_000)
                guard !Task.isCancelled, let self, let task = self.task else { return }
                task.sendPing { [weak self] error in
                    guard error != nil else { return }
                    Task { @MainActor [weak self] in
                        self?.finish("connection lost")
                    }
                }
            }
        }
    }

    private func receiveLoop() {
        // Delegate queue is .main; the next receive is armed only
        // after the previous message is delivered, so order holds.
        task?.receive { [weak self] result in
            MainActor.assumeIsolated {
                guard let self, !self.closed else { return }
                switch result {
                case .success(.string(let text)):
                    self.onMessage?(Data(text.utf8))
                    self.receiveLoop()
                case .success:
                    self.receiveLoop() // binary (visualizer): ignore
                case .failure(let err):
                    self.finish(err.localizedDescription)
                }
            }
        }
    }

    private func finish(_ reason: String) {
        guard !closed else { return }
        close()
        closed = true
        onClose?(reason)
    }
}

extension WebSocketTransport: URLSessionWebSocketDelegate {
    nonisolated func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        // Delegate queue is .main (see open()).
        MainActor.assumeIsolated {
            guard !closed else { return }
            onOpen?()
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
        reason: Data?
    ) {
        MainActor.assumeIsolated {
            finish("the speaker closed the connection")
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        MainActor.assumeIsolated {
            finish(error?.localizedDescription ?? "connection lost")
        }
    }
}
