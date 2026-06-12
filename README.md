# sshup

A terminal UI for getting basic SSH access into a safer shape without having to remember every file and command.

## Mental model

- Generate an SSH key on the client machine you will connect from. The private key stays on that machine.
- Create a non-root user on the server you will connect to, such as `deploy`.
- Put the generated `.pub` key into that server user's `~/.ssh/authorized_keys`.
- Harden sshd on the server after key-based login works. sshup writes a drop-in at `/etc/ssh/sshd_config.d/99-sshup.conf`, validates sshd config with `sshd -t`, then reloads sshd. The managed default SSH port is `2222`.

## Development

```sh
make build
make test
make lint
make release
```
