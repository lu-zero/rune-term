# Roaming sessions with rune-term-agent

A tmux or rmux session on an ssh host normally lasts as long as ssh's
connection. Through rune-term-agent it follows this machine to a new network:
Wi-Fi to tethering, a new address, a laptop that slept. ssh only starts the
agent; QUIC, over UDP, carries the session after that.

The design, the threat model and what comes next are in
[`todo/agent.md`](todo/agent.md).

## How it works

1. rune-term runs `rune-term-agent serve` on the host over ssh. The agent
   binds a UDP port, makes a certificate and a token, prints them in one line,
   and leaves ssh, which returns.
2. rune-term connects to that port. It trusts only that certificate, and the
   agent only a client that shows the token.
3. The agent runs the multiplexer's control mode for the session and joins it
   to the connection. Your session is the multiplexer's, as over ssh: the
   agent holds only the link.
4. When the network changes, QUIC moves the connection. When it is gone for
   longer, a minute without an answer, the group greys and reconnects on its
   own, waiting 1 s and doubling to 32 s between tries.

## Install the agent on a host

The host needs the multiplexer (tmux or rmux) and a Rust toolchain. In a
checkout of rune-term there:

```sh
just agent-install
```

Run it again after updating the checkout. Without `just`:

```sh
cargo install --force --path rune-term-agent --root ~/.local
```

Add `--target-dir DIR` when the checkout's `target` cannot be written.

rune-term starts the agent with `~/.local/bin` and `~/.cargo/bin` added to the
`PATH` of ssh's non-interactive shell, so either install location works
without editing a shell's startup files.

## Use it

- A profile: set the backend to tmux or rmux, a host, and Transport to Agent
  in the profile editor, or in its file:

  ```toml
  backend = "tmux"
  host = "thalia"
  session = "work"
  transport = "agent"
  ```

- The command line: `rune-term attach --roam --host thalia work`, with
  `--rmux` and `--server NAME` as for any attach.

Save as Profile on a roaming group keeps the transport.

## The network

- The agent listens on a random UDP port, and the client reaches it at the
  address ssh reached the host on (`SSH_CONNECTION`). A firewall on the host
  must let UDP in; there is no fixed port range yet.
- The address is the one the host saw ssh arrive on. A host behind a jump host
  (`ProxyJump`), NAT or a port forward advertises an address the client may
  not reach. mosh has the same limit.
- A link that cannot reach the agent reports `no answer from the agent at
  ADDRESS` in the window: UDP is blocked, or the address is not reachable from
  here.

## What is left on the host

- An agent exits a day after its last client, or two minutes after it
  started if no client ever reached it. The session stays with the
  multiplexer either way.
- The token is a credential: whoever holds it can do what your shell on the
  host can. It reaches rune-term only over ssh and is never written to disk.
