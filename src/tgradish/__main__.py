import os
from pathlib import Path
import sys
import shutil

import pydantic
import tomli

from .converter import CmdConfig, convert_video
from .spoofer import spoof_file_duration

DEFAULT_CONFIG_PATH = "./default_config.toml"
CONFIG_PATH = "./config.toml"


def get_best_invocation_for_this_python() -> str:
    """
    Try to figure out the best way to invoke the current Python.
    Taken from pip repository:
    https://github.com/pypa/pip/blob/main/src/pip/_internal/utils/entrypoints.py
    """
    exe = sys.executable
    exe_name = os.path.basename(exe)

    # Try to use the basename, if it's the first executable.
    found_executable = shutil.which(exe_name)
    if found_executable and os.path.samefile(found_executable, exe):
        return exe_name

    # Use the full executable name, because we couldn't find something simpler.
    return exe


def check_file_existence(file_path: str):
    if not Path(file_path).exists():
        print(f"File '{file_path}' is not exists.")
        exit(1)


TGRADISH_CMD = get_best_invocation_for_this_python() + " -m tgradish"


def set_default_config():
    print("Setting default config...")
    shutil.copyfile(DEFAULT_CONFIG_PATH, CONFIG_PATH)
    print("Done.\n")


def get_config() -> CmdConfig:
    if not Path(CONFIG_PATH).exists():
        set_default_config()
    return CmdConfig(**tomli.load(open(CONFIG_PATH, "rb")))


def print_help():
    print(
        f"Usage: {TGRADISH_CMD} [ACTION] [ARGUMENTS]\n",
        "-h, --help          shows this message",
        "[ACTION] --help     shows help for specific action",
        "\nActions:\n",
        "convert   converts source file to videosticker with spoofed duration",
        "spoof     spoofs .webm file duratiob",
        "config    manages tgradish config file",
        sep="\n",
    )


def convert_cmd(args: list[str]):
    config = get_config()

    if not args or args[0] in ("-h", "--help"):
        print(TGRADISH_CMD, "convert [FLAGS]\n")
        config.print_help()
        exit()

    try:
        convert_video(config, args)
        print("Done.")
    except (ValueError, pydantic.error_wrappers.ValidationError) as err:
        print(f"Converter error:\n{err}")
        exit(1)


def spoof_cmd(args: list[str]):
    help_str = TGRADISH_CMD + " spoof [INPUT FILE] [OUTPUT FILE]"

    if not args or args[0] in ("-h", "--help"):
        print(help_str, "\n\nThat's it.")
        exit()
    elif len(args) != 2:
        print("Incorrect arguments format.\n\n", help_str, sep="")
        exit(1)

    input_file, output_file = args
    check_file_existence(input_file)
    spoof_file_duration(input_file, output_file)


def config_cmd(args: list[str]):
    help_str = TGRADISH_CMD + " config [ACTION]"

    if not args or args[0] in ("-h", "--help"):
        print(
            help_str,
            "\nActions:\n",
            "copyfrom [FILE] copies config from file",
            "restore         restores default config",
            "showpath        shows path to config",
            sep="\n",
        )

    elif args[0] == "copyfrom":
        if len(args) != 2:
            print(f"Usage: {TGRADISH_CMD} config copyfrom [FILE]")
            exit(1)
        print(f"Copying config file from {args[1]}...")
        shutil.copyfile(args[1], CONFIG_PATH)
        print("Done.")
        exit()

    elif args[0] == "restore":
        set_default_config()

    elif args[0] == "showpath":
        print(Path(CONFIG_PATH).resolve())

    else:
        print("Invalid config action.")
        exit(1)


def main():
    match sys.argv[1:]:
        case [] | ["-h" | "--help", *_]:
            print_help()
        case ["convert", *args]:
            convert_cmd(args)
        case ["spoof", *args]:
            spoof_cmd(args)
        case ["config", *args]:
            config_cmd(args)


if __name__ == "__main__":
    main()
