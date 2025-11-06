# Unity Integration

This directory contains a Unity package for the NobodyWho project.


## Installation

TBD...

## Dev setup:

requirements: 
- nix
- Unity account (for license)

Unity's licensing is a bit of a pain, so we cant create a properly sandboxed dev environment.
Due to this, we use a Justfile to run the Unity editor with a custom build script.

So to get a environment up and runnign follow these steps:

- Install unityhub.
    1a. Make sure mime handlers are properly setup. For setups using freedesktop / xdg, "x-scheme-handler/unityhub" should be set to "unityhub.desktop"

- Create a Unity account. Give away your personal info, sign away your rights to unity, confirm your email, etc.

- Open Unityhub, log in, and wait for the license file to be created in your home folder. XDG deeplink handler must be set up for this to work. (So installing unityhub in a temporary shell is likely not sufficient).

- Visit https://unity.com/releases/editor/archive and select "6000.2.10f1" from the "All versions" tab.

- Wait for unityhub to open, Install the editor, along with linux build tooling and headless linux build tooling. (XDG deeplink handlers needed again. Be aware if you're running your browser in an environment that doesn't have unityhub deeplink handlers)

This should just be done once, though and then it should be saved to your home folder.

New tests shoudl be add in the src/Tests folder. As long as they are in the Tests namespace, they will be run automatically.


### Just commands

```bash
# run the unity editor with the build script - it will prompt you with instructions for getting the license
# if you do not have one already.
just run-unity

# run the tests
# this creates a temp project and runs the tests in it
just test
```

These commands works by creating a temp project and copying the necessary files into it everytime you run it.





