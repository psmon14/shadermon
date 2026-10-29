Steam allows for background compilation of Vulkan Shaders, which is especially useful on Linux systems, yet they do not offer a progress bar to check on its status, 
unless you are booting up the game that is having its shaders compiled. This simple app should do the trick.
We just tail the shader_log.txt, which shows is progress in percents, the overall amount of shaders, compiled amount and appid.
We can convert appid to an app name by finding its appmanifest file, and since since we know the appid, it's simple.
Functionality is simple:
Show a tray icon that when clicked will show the app name, progress in three ways: percentage, compiled/overall and a progress bar.
Also it sends a notification when a game has finished its shader compilation process.

## Installation and usage

1. Download the AppImage for your distribution from the latest [GitHub Release](https://github.com/xokaiv/shadermon/releases):
   - `Steam_Shader_Monitor-x86_64-Ubuntu.AppImage` for Ubuntu/Debian-based systems.
   - `Steam_Shader_Monitor-x86_64-Arch.AppImage` for Arch-based systems.
2. Open a terminal in the directory containing the downloaded file.
3. Mark it executable (downloads may not retain the executable permission):

```bash
chmod +x Steam_Shader_Monitor-x86_64-Ubuntu.AppImage
```

Use the Arch filename instead when running the Arch build:

```bash
chmod +x Steam_Shader_Monitor-x86_64-Arch.AppImage
```

4. Launch it:

```bash
./Steam_Shader_Monitor-x86_64-*.AppImage
```

If you are on a different Linux distribution, try the release built for the closest family—Ubuntu/Debian or Arch—or build from source.
