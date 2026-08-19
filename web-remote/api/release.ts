// GET /api/release?channel=stable|edge
//
// Caching proxy over the GitHub Releases API for the boxes' update
// checks. Boxes on the edge channel poll every 10 minutes; routed
// through here, a whole fleet costs GitHub roughly one request per
// cache window instead of 144/day/box (anonymous API is 60 req/h/IP).
//
// Deliberately NOT a deployment artifact: this Vercel deploy finishes
// in seconds while the CI image build takes ~10 minutes, so baking a
// version at deploy time would advertise images that don't exist yet.
// Proxying the live release state keeps "a release with assets
// exists" as the single source of truth - the endpoint simply serves
// the previous release until CI actually publishes the next one.
//
// Response is trimmed to the fields boompid's Release deserializer
// reads (tag_name, body, assets[].{name,browser_download_url,size}),
// same field names as GitHub. Downloads still go straight to GitHub's
// CDN - only the metadata lookup is proxied.

const REPO = "TooTallNate/boompi";

export default async function handler(req: any, res: any) {
  const channel = req.query.channel === "stable" ? "stable" : "edge";
  const url =
    channel === "stable"
      ? `https://api.github.com/repos/${REPO}/releases/latest`
      : `https://api.github.com/repos/${REPO}/releases/tags/edge`;

  const gh = await fetch(url, {
    headers: {
      accept: "application/vnd.github+json",
      "user-agent": "boompi-release-proxy",
    },
  });

  if (gh.status === 404) {
    res.setHeader("Cache-Control", "s-maxage=300");
    res.status(404).json({ error: "no release published on this channel yet" });
    return;
  }
  if (!gh.ok) {
    // Rate-limited or GitHub hiccup: short-cache the failure so a
    // polling fleet doesn't stampede, and let boxes fall back to
    // querying GitHub directly.
    res.setHeader("Cache-Control", "s-maxage=60");
    res.status(502).json({ error: `github: HTTP ${gh.status}` });
    return;
  }

  const rel = await gh.json();
  res.setHeader(
    "Cache-Control",
    "s-maxage=300, stale-while-revalidate=86400",
  );
  res.status(200).json({
    tag_name: rel.tag_name,
    body: rel.body ?? "",
    assets: (rel.assets ?? []).map((a: any) => ({
      name: a.name,
      browser_download_url: a.browser_download_url,
      size: a.size,
    })),
  });
}
