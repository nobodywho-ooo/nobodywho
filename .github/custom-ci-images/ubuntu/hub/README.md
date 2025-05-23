# GameCI image providing Unity Hub

#### `unity-ci/hub`

Dockerised Unity Hub made for continuous integration.

## Usage

Run the hub image using an interactive shell

```bash
docker run -it --rm unityci/hub bash
```

#### help

Run the help command

```bash
unity-hub help
```

⚠ Note that the `help`-command currently does not work, but other commands do.

#### install editors

Get a link from the [archive](https://unity3d.com/get-unity/download/archive).

The link `unityhub://2020.1.4f1/fa717bb873ec` holds version `2020.1.4f1` and hash `fa717bb873ec`.

Since we want to install android build support, we'll add `--module android`.

```bash
unity-hub install --version 2020.1.4f1 --changeset fa717bb873ec --module android
```

## License

[MIT license](https://github.com/game-ci/docker/blob/main/LICENSE)
