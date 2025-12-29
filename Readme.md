## Installation

### Linux

Install Nagato using `curl`:

```bash
# Install latest
curl -fsSL https://raw.githubusercontent.com/dannycreations/nagato/main/install.sh | bash
```

```bash
# Install nightly
curl -fsSL https://raw.githubusercontent.com/dannycreations/nagato/main/install.sh | bash -s -- nightly
```

### Windows

Install Nagato using PowerShell:

```powershell
# Install latest
irm https://raw.githubusercontent.com/dannycreations/nagato/main/install.ps1 | iex
```

```powershell
# Install nightly
$env:NAGATO_VERSION='nightly'; irm https://raw.githubusercontent.com/dannycreations/nagato/main/install.ps1 | iex
```

## Manual Installation

You can also download the binaries directly from the [Releases](https://github.com/dannycreations/nagato/releases) page.
