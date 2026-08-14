// Minimal ustar writer: packages the box profile as a boompi-box/
// bundle for provisioning another SD card (the mirror image of
// boompid's tarstream reader and the boot-partition ingest).

function header(name: string, size: number): Uint8Array {
  const block = new Uint8Array(512);
  const ascii = (s: string, offset: number) => {
    for (let i = 0; i < s.length; i++) block[offset + i] = s.charCodeAt(i);
  };
  ascii(name, 0); // name (<= 100 chars: ours are short)
  ascii("0000644\0", 100); // mode
  ascii("0000000\0", 108); // uid
  ascii("0000000\0", 116); // gid
  ascii(size.toString(8).padStart(11, "0") + "\0", 124); // size
  ascii("00000000000\0", 136); // mtime
  block[156] = "0".charCodeAt(0); // typeflag: regular file
  ascii("ustar\0", 257); // magic
  ascii("00", 263); // version
  // Checksum: sum with the checksum field read as spaces.
  block.fill(" ".charCodeAt(0), 148, 156);
  let sum = 0;
  for (const b of block) sum += b;
  ascii(sum.toString(8).padStart(6, "0") + "\0 ", 148);
  return block;
}

export function tarBundle(files: { name: string; content: string }[]): Blob {
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  for (const f of files) {
    const data = enc.encode(f.content);
    parts.push(header(f.name, data.length));
    parts.push(data);
    const pad = (512 - (data.length % 512)) % 512;
    if (pad) parts.push(new Uint8Array(pad));
  }
  parts.push(new Uint8Array(1024)); // end-of-archive
  return new Blob(parts as BlobPart[], { type: "application/x-tar" });
}
