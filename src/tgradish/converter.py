import copy
from pathlib import Path
import subprocess

import pydantic

from .config_model import CmdConfig, CmdParams, ArgsPlaceholder
from . import spoofer


class ArgsQueue:

    def __init__(self, argv):
        self.args: list[str] = argv[::-1]

    def pop_next(self) -> str | None:
        try:
            return self.args.pop()
        except IndexError:
            return None

    def pop_next_n(self, n: int) -> list[str] | None:
        if len(self.args) < n:
            return None
        return [self.args.pop() for _ in range(n)]


class RunInfo(pydantic.BaseModel):
    input: str
    output: str
    guess_value: str | None
    guess_iterations: str
    guess_min: str | None
    guess_max: str | None


def run_ffmpeg_commands(config: CmdConfig, cmd_params: CmdParams):
    cmd_args = cmd_params.args.copy()

    for command in copy.deepcopy(config.passes):
        ph = command.index(ArgsPlaceholder())
        command[ph:ph + 1] = cmd_args
        command = [arg.format(**cmd_params.placeholders) for arg in command]

        print(f"$", *command)
        if returncode := subprocess.run(command).returncode:
            exit(returncode)


def parse_command_args(config: CmdConfig,
                       argv: list[str]) -> dict[str, list[str]]:
    args = ArgsQueue(argv)
    alias_dict = config.map_flag_aliases()

    flag_args_dict: dict[str, list[str]] = {}

    while flag_alias := args.pop_next():
        flag = alias_dict.get(flag_alias)
        if not flag:
            raise ValueError(f"Unknown flag: '{flag_alias}'")

        if (flag_args := args.pop_next_n(flag.nargs)) is None:
            raise ValueError(f"Not enough arguments for a '{flag_alias}'")

        if flag.name in flag_args_dict:
            raise ValueError(f"Duplicate flag: '{flag_alias}'")

        flag_args_dict[flag.name] = flag_args

    return flag_args_dict


def convert_video(config: CmdConfig, argv: list[str]):

    flag_args_dict = parse_command_args(config, argv)
    cmd_params = CmdParams(config)

    for flag_name, flag in config.flag_dict.items():
        flag.parse(flag_name, flag_args_dict.get(flag_name), cmd_params)

    run_info = RunInfo(**cmd_params.placeholders)

    if run_info.guess_value is None:
        run_ffmpeg_commands(config, cmd_params)
        return

    guess_value_flag = config.values.get(run_info.guess_value)

    if guess_value_flag is None:
        raise ValueError(f"Unknown guess value: '{run_info.guess_value}'")

    if guess_value_flag.guess_params is None:
        raise ValueError(f"Value '{run_info.guess_value}' is not guessable")

    params = guess_value_flag.guess_params

    gtype = params.type.get_constructor()
    gmin = gtype(run_info.guess_min or params.min)
    gmax = gtype(run_info.guess_max or params.max)

    try:
        g_it = int(run_info.guess_iterations)
    except ValueError:
        raise ValueError(f"Guess iterations is '{run_info.guess_iterations}',"
                         " should be an integer")

    SIZE_256KB = 1 << 18

    output_path = Path(cmd_params.placeholders["output"])
    tmp_dir = output_path.parent / "tmp"
    tmp_dir.mkdir(exist_ok=True)

    best_valid_option = None

    for i in range(g_it):
        mean_value = gtype((gmin + gmax) / 2)
        fvalue = params.type.format(mean_value)
        cmd_params_i = copy.deepcopy(cmd_params)
        guess_value_flag.parse(run_info.guess_value, [fvalue], cmd_params_i)

        path = (tmp_dir / output_path.name).with_suffix(f".v_{fvalue}.webm")
        cmd_params_i.placeholders["output"] = str(path)

        print(f"\nIteration №{i + 1}, running ffmpeg"
              f" with {run_info.guess_value} = {fvalue}...")
        run_ffmpeg_commands(config, cmd_params_i)
        result_size = path.stat().st_size

        if result_size == SIZE_256KB:
            print(f"Result is ideal size.\n")
            best_valid_option = path
            break
        elif result_size < SIZE_256KB:
            print(f"Result is smaller than limit, increasing lower limit...\n")
            best_valid_option = path
            cut_from = "below"
        else:
            print(f"Result is bigger than limit, decreasing upper limit...\n")
            cut_from = "above"

        if (cut_from == "below") ^ params.scaling.is_inverse():
            gmin = mean_value
        else:
            gmax = mean_value

    if best_valid_option is None:
        print("Script did not find settings for a sufficiently small final"
              " file, try increasing the number of iterations"
              " or reducing the quality settings")
        exit(1)

    spoofer.spoof_file_duration(best_valid_option, run_info.output)
    