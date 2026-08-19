---
"boompi": patch
---

Update checks route through boompi.n8.io first. The hosted remote
gained a caching endpoint (/api/release) that proxies the GitHub
release lookup, so a fleet of edge boxes polling every 10 minutes
costs GitHub roughly one request per cache window instead of one per
box. It proxies live release state rather than baking a version at
deploy time - Vercel deploys in seconds while the image build takes
minutes, and this way "a release with assets exists" stays the single
source of truth, no reconciliation needed. The endpoint is a cache,
not a dependency: any failure and the box asks GitHub directly, like
before. Downloads were never proxied - they go straight to GitHub's
CDN either way.
