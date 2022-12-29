from collections import Counter, ChainMap
import itertools
from pathlib import Path
from typing import ClassVar, Iterable, TypeVar

import pydantic
import enum

T = TypeVar("T")


def get_duplicates_list(l: Iterable[T]) -> list[T]:
    return [item for item, count in Counter(l).items() if count > 1]


class CmdParams:

    def __init__(self, config: "CmdConfig") -> None:
        self.args: list[str] = config.required_args
        self.placeholders: dict[str, str] = {}

    def set_default_output(self):
        if "output" not in self.placeholders:
            output = Path(self.placeholders["input"]).with_suffix(".webm")
            print(f"Output file defaults to '{output}'.")
            self.placeholders["output"] = str(output)


class CmdFlag(pydantic.BaseModel):
    name: str = "..."  # post-initialized in the CmdConfig._add_flag_names
    description: str
    aliases: list[str]
    nargs: ClassVar[int]

    class Config:
        extra = "forbid"

    def get_flag_alias_dict(self) -> dict[str, "CmdFlag"]:
        return dict.fromkeys(self.aliases, self)

    def parse(self, flag_name: str, flag_args: list[str] | None,
              cmd_params: CmdParams) -> None:
        raise NotImplementedError

    def _print_flags_and_description(self):
        flags = ", ".join(self.aliases)
        print(f"{flags:<23} {self.description}")

    def print_help(self):
        self._print_flags_and_description()


class ArgsPlaceholder(pydantic.BaseModel):

    def format(self, **_kwargs):
        raise NotImplementedError

    class Config:
        extra = "forbid"


class EnumOption(pydantic.BaseModel):
    description: str
    default: bool = False
    args: list[str] = []
    placeholders: dict[str, str] = {}

    def apply(self, cmd_params: CmdParams):
        cmd_params.args += self.args
        cmd_params.placeholders |= self.placeholders

    def print_help(self, name):
        default_marker = "(default)" if self.default else ""
        print(f"    {name:<23} {self.description} {default_marker}")


class EnumFlag(CmdFlag):
    options: dict[str, EnumOption]
    nargs: ClassVar[int] = 1

    @pydantic.root_validator
    def only_one_default_option(cls, values):
        options: dict[str, EnumOption] = values.get("options")
        defaults = [name for name, option in options.items() if option.default]
        if not defaults:
            raise ValueError(f"There is no defult options"
                             f" in '{values.get('name')}' flag, should be 1")

        if (n := len(defaults)) > 1:
            raise ValueError(f"There is {n} default options"
                             f" in '{values.get('name')}' flag:"
                             f" {', '.join(defaults)}, should be only 1")
        return values

    @property
    def default_option(self) -> EnumOption:
        for option in self.options.values():
            if option.default:
                return option
        raise  # default option is always present

    def parse(self, flag_name, flag_args, cmd_params):
        if not flag_args:
            self.default_option.apply(cmd_params)
            return

        option_name = flag_args[0]
        option = self.options.get(option_name)
        if not option:
            raise ValueError(f"Unknown '{flag_name}' option: '{option_name}'")

        option.apply(cmd_params)

    def print_help(self):
        print()
        self._print_flags_and_description()
        for option_name, option in self.options.items():
            option.print_help(option_name)


class SwitchFlag(CmdFlag):
    inverted: bool = False
    args: list[str]
    nargs: ClassVar[int] = 0

    def parse(self, _flag_name, flag_args, cmd_params):
        if (flag_args == []) ^ self.inverted:
            cmd_params.args += self.args


class ValueType(enum.Enum):
    INT = "int"
    FLOAT = "float"

    def get_constructor(self):
        if self == self.INT:
            return int
        return float

    def format(self, n: int | float):
        if self == self.INT:
            return f"{int(n):d}"
        return f"{n:.2f}"


class Scaling(enum.Enum):
    DIRECT = "direct"
    INVERSE = "inverse"

    def is_inverse(self):
        return self == self.INVERSE


class GuessParams(pydantic.BaseModel):
    min: int | float
    max: int | float
    type: ValueType
    scaling: Scaling


class ValueFlag(CmdFlag):
    args: list[str] = []
    optional: bool = False
    default_value: str | None
    guess_params: GuessParams | None = None
    nargs: ClassVar[int] = 1

    def parse(self, flag_name, flag_args, cmd_params):
        if not flag_args:
            if not self.optional:
                raise ValueError(f"Required value '{flag_name}'"
                                 " is not present")
            if self.default_value is not None:
                cmd_params.args += self.args
                cmd_params.placeholders[flag_name] = self.default_value
            return

        cmd_params.args += self.args
        cmd_params.placeholders[flag_name] = flag_args[0]

    def print_help(self):
        flags = ", ".join(self.aliases)
        guessable_marker = "(guessable)" if self.guess_params else ""
        print(f"{flags:<23} {self.description} {guessable_marker}")


class CmdConfig(pydantic.BaseModel):
    passes: list[list[str | ArgsPlaceholder]]
    required_args: list[str]
    enums: dict[str, EnumFlag] = {}
    switches: dict[str, SwitchFlag] = {}
    values: dict[str, ValueFlag] = {}

    @pydantic.root_validator
    def _add_flag_names(cls, values: dict[str, dict[str, CmdFlag]]):
        for flag_dict_name in ("enums", "switches", "values"):
            for name, cmdflag in values[flag_dict_name].items():
                cmdflag.name = name
        return values

    @property
    def flag_dicts(self) -> tuple[dict[str, CmdFlag], ...]:
        return (self.enums, self.switches, self.values)  # type: ignore

    @property
    def flag_dict(self) -> dict[str, CmdFlag]:
        flag_name_list = itertools.chain(*map(dict.keys, self.flag_dicts))

        if duplicates := get_duplicates_list(flag_name_list):
            raise ValueError(f"Duplicate flag names: {', '.join(duplicates)}.")

        return dict(ChainMap(*self.flag_dicts))

    def map_flag_aliases(self) -> dict[str, CmdFlag]:
        alias_dict = {}

        for flag_name, flag in self.flag_dict.items():
            for alias in flag.aliases:
                if alias in alias_dict:
                    raise ValueError(f"Duplicate alias '{alias}'"
                                     f" for the {flag_name} flag")

            alias_dict.update(flag.get_flag_alias_dict())

        return alias_dict

    def print_help(self):
        gvalues_list = [i.name for i in self.values.values() if i.guess_params]
        gvalues_list.append("none")
        guessable_values = "\n\nGuessable values: " + ", ".join(gvalues_list)

        section_headers = (
            "Enum flags. Usage: '--flag <OPTION>'",
            "Switch flags. Usage: '--flag' to turn on",
            "Value flags. Usage: '--flag <VALUE>'" + guessable_values,
        )

        for section, header in zip(self.flag_dicts, section_headers):
            print(f"\n{header}\n")
            for flag in section.values():
                flag.print_help()
            print()
