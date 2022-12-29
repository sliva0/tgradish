# tgradish

Simple video converting cli utility specializing in Telegram videostickers with the ability to bypass the 3 second limit.


## Installation

``` console
python -m pip install tgradish
```


## Dependencies

For the `tgradish convert` command `ffmpeg` should be installed in PATH,
but `tgradish spoof` works just fine even without it.


## Usage examples

Converting .mp4 video to videosticker:
``` console
$ tgradish convert -i ~/pig.mp4
```

Spoofing duration of already correctly encoded videosticker:
``` console
$ tgradish spoof ~/pig.webm ~/spoofed_pig.webm
```


## License

[MIT License](LICENSE.txt)
