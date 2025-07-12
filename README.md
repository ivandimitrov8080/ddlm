# Not (so) Dumb Login Manager



A stupidly simple graphical login manager. 
Uses framebuffer, so You wont have to run a wayland session to bootstrap your wayland session (unlike gtkgreet)

This is a greetd frontend.

# Prior work:
Continuation/fork of prior work by [deathowl](https://github.com/deathowl/ddlm).
Also supports plymouth themes and additional config,
because the year of the linux desktop won't come before we have smooth boot screens.

# Setup

> My greetd config looks like :
> ```
> # The default session, also known as the greeter.
> [default_session]

> command = "ndlm --target /usr/bin/sway " 

> # The user to run the command as. The privileges this user must have depends
> # on the greeter. A graphical greeter may for example require the user to be
> # in the `video` group.
> user = "greetd"
> ```
For this one check flake.nix#nixosConfigurations.default
To see it for yourself `nix run`

# Future plans:
* Enable selection of WM on the login screen
* Support a larger portion of plymouth theming
