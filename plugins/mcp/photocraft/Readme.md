# PhotoCraft MCP Integration

Headless and live-bridge image editing engine for `rs-agent` via the Model Context Protocol (MCP).

---

## 1. Installation

### Arch Linux / CachyOS (AUR)
```bash
paru -S photocraft
# or
yay -S photocraft
```

### Build from Source
```bash
git clone https://github.com/storytold/photocraft.git /tmp/photocraft
cd /tmp/photocraft
cargo build --release -p photocraft -p photocraft-cli
install -m 755 target/release/photocraft target/release/photocraft-cli ~/.local/bin/
cd ~ && rm -rf /tmp/photocraft
```

---

## 2. Authentication Token Setup

PhotoCraft requires a 64-character hex control token for socket authentication:

```bash
mkdir -p ~/.config/photocraft
openssl rand -hex 32 > ~/.config/photocraft/control.token
chmod 600 ~/.config/photocraft/control.token
```

---

## 3. Verification

Verify that rs-agent connects to PhotoCraft:

```bash
ai mcp list photocraft
```
