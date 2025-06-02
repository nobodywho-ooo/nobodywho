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
The unity executable is downloaded by the `nix develop` command. But the license needs to be downloaded manually.

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





