# A maintained and updated fork of [Giuroll](https://github.com/Giufinn/giuroll)

Is a network rollback mod for 東方非想天則 / Touhou 12.3 Hisoutensoku, which aims to significantly improve the responsiveness of netplay, as well as introducing other rollback related improvements to replay mode.  

Currently this is an early version, and might slightly increase instability, but will still significantly improve the netplay experience for almost all connections.  

This repository also contains a stripped down version of the crate [ilhook-rs](https://github.com/regomne/ilhook-rs), and a modified version of [mininip](https://github.com/SlooowAndFurious/mininip), all rights remain with their respective authors.

## Usage  

### For [SWRSToys](https://github.com/SokuDev/SokuMods/) users
1. Navigate to your Hisoutensoku folder. You should see a subfolder called `modules`, and a file called `SWRSToys.ini`.
2. Drop the giuroll folder from this zip into `modules`. 
3. Add the following line into your `SWRSTOYS.ini`
```
giuroll=modules/giuroll/giuroll.dll
```
4. Find the following line
`SWRSokuRoll=modules/SWRSokuRoll/SWRSokuRoll.dll`.
5. Add a `;` at the beginning of that line, making it
```
; SWRSokuRoll=modules/SWRSokuRoll/SWRSokuRoll.dll
```

### For users without SWRSToys
See the [Injector](https://github.com/Hagb/giuroll-injector/tree/main).  

**More information about the usage in game is available in the `installation and usage.txt` file inside the distributed zip**

## Replay Rewind  

In replay mode pressing `q` will start rewinding the replay, and the `A`, `S` and `D` keys will affect the rewind speed.  
Pressing `z` will pause the replay, allowing you to move frame by frame, backwards or forwards via the `s` and `d` keys respectively.

## Usage with [Tsk](https://wikiwiki.jp/thtools/%E5%A4%A9%E5%89%87%E8%A6%B3)

When used with Giuroll, Tsk may record one battle multiple times. A workaround is provided for it:

To apply the workaround, `SWRSAddr.ini` of tsk should be edited: delete the original line which starts with `SWRS_ADDR_PBATTLEMGR`, or make it a comment by add a semicolon `;` at the beginning of it, and then add the following line (also on the `SWRSAddress` section):

``` ini
SWRS_ADDR_PBATTLEMGR = 0x0047579c
```

## Building from source
The mod can be buit with `cargo` using the commands below.
```bash
rustup default nightly-2024-06-18
rustup component add rust-src --toolchain nightly-2024-06-18
cargo +nightly-2024-06-18 build --target i686-win7-windows-msvc -Z build-std --release
```
For debugging/developmental purposes, you may build with the `--release` flag omitted. This will open a console window and show further details while the game is running. 
<!--When building from source please remember to add the `--release`/`-r` flag.-->

## Common Problems  

- Game doesn't load: check if the ini is valid according to the example ini provided in this repository, and is placed alongside the mod without any changes to it's name, and check for mod conflicts by disabling all other mods, and adding them back one by one.  
- Failed to connect: either player is using an incompatible version of giuroll, or is not using it at all.  
- Game desynced: I'm planning on adding a desync detector to make debugging desyncs easier, but since desyncs also occur with SokuRoll there is no guarantee they are caused solely by the rollback. If the desyncs are common, persists between game restarts, and are not appearing with Sokuroll, you can contact me about it.

You can send feedback about any issues through:
- [GitHub issues](https://github.com/Hagb/giuroll-hagb/issues), and/or
- `@hagb_` in DMs or in a [hisoutensoku server](https://hisouten.koumakan.jp/wiki/Discord_Servers_List) through [Discord](https://discord.com).

### Special thanks to:

[Giufin](https://github.com/Giufinn) - for developing the oringinal [Giuroll](https://github.com/Giufinn/giuroll), and advice, support and other help in the development of this fork.

[DPhoenix](https://github.com/enebe-nb) and [PinkySmile](https://github.com/Gegel85) - for advice and support with hisoutensoku modding  

Ysaron - for majority of the testing 

TStar, Barcode, Rouen, ChèvreDeFeu, Klempfer, LunaTriv, Aquatic, BIG BREWED and Sigets - for additional testing

[Slavfox](https://github.com/slavfox) - for various help with reverse engineering and open source

Fireseal - for making the original rollback mod for hisoutensoku
