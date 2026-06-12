# Making Secure SSH Stupid Simple

SSH is an area where security matters. We need a simple way for users to get ssh up and running on a new system and generate keys while being confident that things are secure and also a system that guides the user on how to access the system from other systems.

# Assistance In User Creation

When managing ssh, we want to make sure we are not providing root access unless explicitly asked. Because of this, we want to make sure that this system also provides an easy way to create new users on the system.

# Ratatui TUI

This system is a tui system meaning it does not need a traditional UI to operate. We are writing this in rust and we want to make use of the ratatui tui library provided by the community.

# Makefile

We want a makefile where we can simply run make install. That should help us get up and running quickly without any hassle.