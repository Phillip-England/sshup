# sshup

A terminal UI for getting basic SSH access into a safer shape without having to remember every file and command.

## Quickstart

This guide sets up SSH on a server, creates a non-root login user, connects with an SSH key from your client machine, moves SSH to port `2222`, opens that port in `ufw`, and installs `fail2ban`.

You will use two machines:

- Client: the laptop or workstation you connect from.
- Server: the machine you want to SSH into.

### 1. Install sshup

On the client and on the server, install Rust if needed, then run:

```sh
make install
```

This installs the `sshup` command into `/usr/local/bin` by default.

### 2. Generate a key on the client

On the client machine, run:

```sh
sshup
```

Choose `Generate an Ed25519 SSH key`.

The default key path is:

```text
~/.ssh/id_ed25519_sshup
```

Keep the private key on the client. You will copy only the public key, which ends in `.pub`, to the server.

Show the public key:

```sh
cat ~/.ssh/id_ed25519_sshup.pub
```

### 3. Install SSH server software on the server

On the server, run:

```sh
sshup
```

Choose `Install OpenSSH server`.

sshup will use the system package manager when it recognizes one, then it will try to enable and start the SSH service.

### 4. Create a non-root SSH user on the server

Still on the server, choose `Create a non-root SSH user`.

The default username is:

```text
deploy
```

Set a password when prompted. This user is the account you will connect to over SSH.

### 5. Add your client public key to the server user

On the server, create the SSH directory for the new user:

```sh
sudo install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
```

Open the authorized keys file:

```sh
sudo nano /home/deploy/.ssh/authorized_keys
```

Paste the full public key from the client. It should be one long line starting with `ssh-ed25519`.

Save the file, then secure it:

```sh
sudo chown deploy:deploy /home/deploy/.ssh/authorized_keys
sudo chmod 600 /home/deploy/.ssh/authorized_keys
```

### 6. Test SSH before hardening

From the client, test the connection on the current SSH port:

```sh
ssh -i ~/.ssh/id_ed25519_sshup deploy@SERVER_IP_OR_HOSTNAME
```

Replace `SERVER_IP_OR_HOSTNAME` with the server IP address or DNS name.

Do not harden SSH until this key-based login works.

### 7. Harden SSH and move it to port 2222

On the server, run:

```sh
sshup
```

Choose `Install hardened sshd settings on port 2222`.

sshup writes `/etc/ssh/sshd_config.d/99-sshup.conf`, validates the SSH config, then reloads sshd.

### 8. Open port 2222 in ufw

On the server, run `sshup` and choose `Allow SSH through ufw`.

Use port:

```text
2222
```

sshup adds the allow rule but does not enable `ufw` automatically. That avoids blocking other services by surprise.

If you have reviewed your other server ports and are ready to enable ufw, run:

```sh
sudo ufw status verbose
sudo ufw enable
```

### 9. Install fail2ban

On the server, run `sshup` and choose `Install fail2ban`.

sshup installs the package where supported and tries to enable and start the `fail2ban` service.

### 10. Connect on the hardened port

From the client, connect with:

```sh
ssh -i ~/.ssh/id_ed25519_sshup -p 2222 deploy@SERVER_IP_OR_HOSTNAME
```

You can also run `sshup` on the client and choose `Show connection command` to build this command interactively.

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
