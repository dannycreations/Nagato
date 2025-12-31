## Installation

### Linux

Install Nagato using `curl`:

```bash
# Install latest
curl -fsSL https://bit.ly/nagato-linux | bash
```

```bash
# Install nightly
curl -fsSL https://bit.ly/nagato-linux | bash -s -- nightly
```

### Windows

Install Nagato using PowerShell:

```powershell
# Install latest
irm https://bit.ly/nagato-windows | iex
```

```powershell
# Install nightly
$env:NAGATO_VERSION='nightly'; irm https://bit.ly/nagato-windows | iex
```

## Manual Installation

You can also download the binaries directly from the [Releases](https://github.com/dannycreations/nagato/releases) page.
