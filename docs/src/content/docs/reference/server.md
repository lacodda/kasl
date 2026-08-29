---
title: "server"
---

The `server` command connects this machine to a [kasl-server](https://github.com/lacodda/kasl-server) - the team server that collects workdays from everyone's agent and gives a manager a dashboard.

Connecting is optional. kasl works entirely on its own, and nothing in this command changes that: the local database stays the record of your day either way.

## Usage

```bash
kasl server <SUBCOMMAND>
```

| Subcommand | Purpose |
| --- | --- |
| `connect` | Connect this machine with an agent token |
| `status` | Show the connection and whether it still works |
| `disconnect` | Forget the connection and remove the stored token |

## Getting a token

You do not issue your own token. An administrator creates one on the server - under the person it reports for, labelled with the machine it belongs to - and hands it over. The server shows a token once, at issue, and stores only its hash afterwards, so a lost token is replaced rather than recovered.

## `kasl server connect`

```bash
kasl server connect [OPTIONS]
```

- `--url <URL>`: Server address, e.g. `https://kasl.example.com`. Prompted for when omitted.
- `--ca-certificate <PATH>`: PEM file holding the certificate authority that signed the server's certificate.

The token is always asked for at the prompt, never taken from an argument: a token on a command line ends up in shell history.

```console
$ kasl server connect
kasl-server URL: https://kasl.example.com
https://kasl.example.com is kasl-server vX.Y.Z
Agent token (issued by your administrator): ****************
Connected as Kirill Lakhtachev (agent 'laptop')
```

### What it checks, and why in that order

**First, that the address is a kasl-server.** A URL is asked about before the token is, so a typo in the address is reported as a typo rather than as a rejected token - two different problems with two different fixes.

**Then, whose token it is.** The server is asked who it thinks is connecting, and the answer is printed. A token is an opaque string; one pasted from the wrong chat window works perfectly and quietly files this machine's days under a colleague's name. The check costs one request and puts the mistake in front of a person while they are still looking.

Nothing is written until both checks pass. A half-written connection - an address saved with a token the server never accepted - would leave the agent looking configured while every upload failed.

### Where the token is kept

In the operating system's credential store: Credential Manager on Windows, Keychain on macOS, the Secret Service on Linux - the same place as every other kasl credential (see [ADR 0001](https://github.com/lacodda/kasl/blob/main/docs/adr/0001-os-keyring.md)). The config file records the address only, so it stays safe to read, copy between machines, and paste into a bug report.

### Self-hosted certificates

A company kasl-server is routinely behind a private CA or a self-signed certificate, which the system trust store does not know about. Name the CA that signed it:

```bash
kasl server connect --url https://kasl.internal --ca-certificate /etc/ssl/company-ca.pem
```

The certificate is *added* to the trust store rather than replacing it, so public certificates keep working. There is deliberately no option to turn verification off: that would also accept anyone else's certificate, on a connection carrying a bearer token.

The scheme is never guessed. `http://` and `https://` differ by whether the token crosses the network in the clear, which is not a default worth inventing on your behalf.

## `kasl server status`

```bash
kasl server status
```

Reaches the server rather than reading the config back, because what you run this to find out is whether the connection still works:

```console
$ kasl server status
Configured server: https://kasl.example.com
https://kasl.example.com is kasl-server vX.Y.Z
Connected as Kirill Lakhtachev (agent 'laptop')
```

Each part can fail on its own, and each failure names its own fix - the server unreachable, the token no longer accepted (revoked, or the account deactivated), or a configured server with no token stored, which means connecting again.

## `kasl server disconnect`

```bash
kasl server disconnect
```

Removes the stored token, then the configured address - in that order, so a failure never leaves a working credential behind with nothing pointing at it. Disconnecting does not revoke the token on the server; ask an administrator to revoke it if the machine is being handed on.

Local data is untouched. Days already uploaded stay on the server, and everything in the local database stays where it is.

## Examples

```bash
# Connect, answering the prompts
kasl server connect

# Connect to a known address without the URL prompt
kasl server connect --url https://kasl.example.com

# Connect to a server behind a company CA
kasl server connect --url https://kasl.internal --ca-certificate /etc/ssl/company-ca.pem

# Check where the connection stands
kasl server status

# Forget the connection on a machine being handed on
kasl server disconnect
```

## Related commands

- **[`report`](/reference/report/)** - The daily report, which the corporate `si` integration submits separately
- **[`setup`](/reference/setup/)** - Configure the rest of kasl, including that separate reporting API
