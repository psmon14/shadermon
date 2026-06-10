Steam allows for background compilation of Vulkan Shaders, which is especially useful on Linux systems, yet they do not offer a progress bar to check on its status, 
unless you are booting up the game that is having its shaders compiled. This simple app should do the trick.
We just tail the shader_log.txt, which shows is progress in percents, the overall amount of shaders, compiled amount and appid.
We can convert appid to an app name by finding its appmanifest file, and since since we know the appid, it's simple.
Functionality is simple:
Show a tray icon that when clicked will show the app name, progress in three ways: percentage, compiled/overall and a progress bar.
Also it sends a notification when a game has finished its shader compilation process.
