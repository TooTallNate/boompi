# Per-box binaries live on /data/bin (survives OS updates; not part
# of the image). First in PATH so a box can override image binaries.
export PATH="/data/bin:$PATH"
