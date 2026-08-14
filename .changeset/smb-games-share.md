---
"boompi": minor
---

Each box now offers a guest SMB network share of its games library -
drag ROMs straight from Finder or Explorer onto smb://<box>/games,
no credentials. The share is scoped to /data/games and nothing else:
ssh keys, Wi-Fi credentials, and the box profile are outside the
exported tree by construction (a build assertion keeps it that way).
The boxes appear in the network sidebar automatically via mDNS.
macOS metadata droppings are vetoed server-side and ignored by the
library scanner.
