# Unity-CI image of the Unity Editor

#### `unity-ci/editor`

Dockerised Unity Editor made for continuous integration.

## Usage

Run the editor image using an interactive shell

```bash
docker run -it --rm unityci/editor:[tag] bash
```
example

```bash
docker run -it --rm unityci/editor:ubuntu-2020.1.1f1-android-0.3.0 bash
```


Run the editor 

```bash
unity-editor help
```

⚠ Note that the `help`-command currently does not work, but other commands do.

## License

[MIT license](https://github.com/game-ci/docker/blob/main/LICENSE)

