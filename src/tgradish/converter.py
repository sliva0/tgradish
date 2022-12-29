import copy
from pathlib import Path
import subprocess

import pydantic

from .config_model import CmdConfig, CmdParams, ArgsPlaceholder
from . import spoofer

SIZE_256KB = 1 << 18


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
    guess_value: str
    guess_iterations: str
    guess_min: str | None
    guess_max: str | None


def generate_tmp_file_path(output: str, suffix: str = ""):
    output_path = Path(output)
    tmp_dir = output_path.parent / "tmp"
    tmp_dir.mkdir(exist_ok=True)
    return (tmp_dir / output_path.name).with_suffix(f".i{suffix}.webm")


def run_ffmpeg_commands(config: CmdConfig, params: CmdParams, output: Path):
    for command in map(list.copy, config.passes):
        ph = command.index(ArgsPlaceholder())
        command[ph:ph + 1] = params.args
        params.placeholders["output"] = str(output)

        command = [arg.format(**params.placeholders) for arg in command]

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


def guess_value(config: CmdConfig, cmd_params: CmdParams, run_info: RunInfo):
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

    best_valid_option = None

    print(f"Starting to guess {run_info.guess_value} on interval"
          f" [{gmin}, {gmax}] in {g_it} iterations...")

    for i in range(g_it):
        mean_value = gtype((gmin + gmax) / 2)
        fvalue = params.type.format(mean_value)
        cmd_params_i = copy.deepcopy(cmd_params)
        guess_value_flag.parse(run_info.guess_value, [fvalue], cmd_params_i)

        path = generate_tmp_file_path(run_info.output, f"_{fvalue}")

        print(f"\nIteration №{i + 1}, running ffmpeg"
              f" with {run_info.guess_value} = {fvalue}:")
        run_ffmpeg_commands(config, cmd_params_i, output=path)
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
              " or reducing the quality settings.")
        exit(1)

    return best_valid_option


def convert_video(config: CmdConfig, argv: list[str]):
    flag_args_dict = parse_command_args(config, argv)
    cmd_params = CmdParams(config)

    for flag_name, flag in config.flag_dict.items():
        flag.parse(flag_name, flag_args_dict.get(flag_name), cmd_params)

    cmd_params.set_default_output()
    run_info = RunInfo(**cmd_params.placeholders)

    if run_info.guess_value == "none":
        output_file = generate_tmp_file_path(run_info.output)
        run_ffmpeg_commands(config, cmd_params, output=output_file)
    else:
        output_file = guess_value(config, cmd_params, run_info)

    efficiency = output_file.stat().st_size / SIZE_256KB
    print(f"Final file uses {efficiency:.2%}"
          " of avaliable sticker file size.")

    spoofer.spoof_file_duration(output_file, run_info.output)