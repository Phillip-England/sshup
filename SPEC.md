# Making Secure SSH Stupid Simple

SSH is an area where security matters. We need a simple way for users to get ssh up and running on a new system and generate keys while being confident that things are secure and also a system that guides the user on how to access the system from other systems.

# Assistance In User Creation

When managing ssh, we want to make sure we are not providing root access unless explicitly asked. Because of this, we want to make sure that this system also provides an easy way to create new users on the system.

# Ratatui TUI

This system is a tui system meaning it does not need a traditional UI to operate. We are writing this in rust and we want to make use of the ratatui tui library provided by the community.

# Makefile

We want a makefile where we can simply run make install. That should help us get up and running quickly without any hassle.

We also need a command called make release which will generate all the binaries we need so that users can just pull down raw binaries for their given system.

When running make release, binaries for most major platforms will be built and stashed in a directory. This allows other users to pull down this program without installing rust.

# Simple UI

We want the UI for focus on simplicity, pragmatism, and clear explanations. When we do something on the system, we want it to be very clear what is happening and what has occurred.

# Fail2ban Installation

This system should offer a way to get fail2ban up and running on the users system as this project cares about security. The tui system should offer this options.

# Clear Indication of Active ssh

We want to see very clearly in the tui if ssh is running on our system or if it is not running. sshup should server the purpose of showing the user if ssh us running and which port it is running on.

# Port 2222 by Default

Since this system cares about security, we need to not run on port 22, but instead run on port 2222 by default. However, we should be able to easily change which port ssh is running on and it should show in the tui which port ssh is running on.

# ufw Firewall Support

We should support ufw firewall (or another firewall that is more universal if that makes more sense). The main point is we need some sort os system to ensure we can establish nice firewall settings.

When establishing these firewall settings, it should be done in such a way as to not disrupt other applications on the system. For example, getting the firewall setup for ssh should not imped my ability to run servers on the same machine. 

If doing this is not possible, then opt out of firewall support all together. It might make more sense to establish firewall settings it is own tui application, and if so, please let me know.

# Help With Installation of ssh Software

This system should not assume the user has ssh server installed. You should offer a way for them to install it very easily using this system as well. That way this system is a one-stop-shop for all ssh needs, including installation.