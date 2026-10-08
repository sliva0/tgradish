from contextlib import contextmanager
import ctypes
import math
from typing import BinaryIO


class SpoofError(Exception):
    pass


# tuple of Element IDs from .xml specfication:
# https://github.com/ietf-wg-cellar/matroska-specification/blob/master/ebml_matroska.xml
# paths: \Segment, \Segment\Info, \Segment\Info\Duration
VINT_DURATION_PATH = [0x18538067, 0x1549A966, 0x4489]

# Max size of VINT_WIDTH and VINT_MARKER parts, in bytes.
VINT_MAX_WIDTH = 1


def float_to_n_bytes(number: float, size: int) -> bytes:
    """
    Generates binary representation of float with given byte size,
    following RFC 8794, section 7.3:
    https://www.rfc-editor.org/rfc/rfc8794.html#name-float-element
    """
    match size:
        case 0:
            return b''
        case 4:
            c_value = ctypes.c_float(number)
        case 8:
            c_value = ctypes.c_double(number)
        case _:
            raise ValueError('Impossible float size')
    return bytes(c_value)[::-1]


def vint_to_bytes(vint: int) -> bytes:
    """
    Converts VINT value to bytes form.
    """
    return vint.to_bytes(math.ceil(vint.bit_length() / 8), 'big')


def bytes_to_bit_str(data: memoryview | bytes) -> str:
    """
    Converts bytes to the str of '0' and '1'.
    """
    nbytes = len(data)
    return format(int.from_bytes(data, 'big'), f'0{nbytes * 8}b')


def vint_bytes_to_int(data: memoryview | bytes) -> tuple[int, int]:
    """
    Reads Variable-Size Integer from the start of the data
    according to RFC 8794 section 4:
    https://www.rfc-editor.org/rfc/rfc8794.html#name-variable-size-integer

    Returns VINT length in bytes and its value.
    """
    try:
        vint_length = bytes_to_bit_str(data[:VINT_MAX_WIDTH]).index('1') + 1
    except IndexError:
        raise IndexError('VINT_MARKER not found.') from None
    vint_value = int(bytes_to_bit_str(data[:vint_length])[vint_length:], 2)
    return vint_length, vint_value


def vint_to_int(vint: int) -> int:
    """
    Converts VINT value to normal int.
    """
    return vint_bytes_to_int(vint_to_bytes(vint))[1]


def normalize_path(path: list[int]) -> list[int]:
    """
    Converts Element IDs in the path into int form.
    """
    return [vint_to_int(vint) for vint in path]


def enter_element(data: memoryview) -> memoryview:
    """
    Parses Element Data Size from the beginning of the memoryview
    and returns memoryview of the parsed element.
    """
    vint_len, element_len = vint_bytes_to_int(data)
    return data[vint_len : vint_len + element_len]


def skip_element(data: memoryview) -> memoryview:
    """
    Parses Element Data Size from the beginning of the memoryview
    and returns memoryview of the next elements, skipping current one.
    """
    vint_len, element_len = vint_bytes_to_int(data)
    return data[vint_len + element_len :]


def find_value_by_path(data: memoryview, el_id_path: list[int]) -> memoryview:
    """
    Finds element in the EBML Data with given Element ID path to it.
    Returns memoryview of found element.

    Works by parsing EBML data by parsing Elements ID at the start of the
    memoryview and comparing it to the one in the Element ID path. Skips
    elements if it's not equal and enters if it is, also advancing
    path index.
    """
    path: list[int] = normalize_path(el_id_path)

    while data:
        vint_len, element_id = vint_bytes_to_int(data)
        data = data[vint_len:]

        if element_id == path[0]:
            data = enter_element(data)
            path.pop(0)
            if not path:
                return data
        else:
            data = skip_element(data)
    else:
        raise IndexError(f'Element with ID: 0x{path[0]:x} was not found.')


@contextmanager
def spoofer_exception_wrapping():
    try:
        yield
    except (IndexError, ValueError) as err:
        raise SpoofError(
            'Duration spoofing failed due to incorrectly'
            f' encoded file. Some additional info: {err}'
        )


# REWRITE CODE BELOW


def get_funny_bytes_and_index_for_it(
    data: memoryview, funny_number: float
) -> tuple[int, int, bytes]:
    try:
        start, end = find_value_by_path(data, VINT_DURATION_PATH)
        funny_bytes = float_to_n_bytes(funny_number, size=end - start)
    except (IndexError, ValueError) as err:
        raise SpoofError(
            'Duration spoofing failed due to incorrectly'
            f' encoded file. Some additional info: {err}'
        )
    return start, end, funny_bytes


def spoof_memoryview_duration(data: memoryview, funny_number: float):
    start, end, funny_bytes = get_funny_bytes_and_index_for_it(
        data, funny_number
    )
    data[start:end] = funny_bytes


def spoof_file_duration(file: BinaryIO, funny_number: float):
    data = memoryview(file.read())
    start, _, funny_bytes = get_funny_bytes_and_index_for_it(data, funny_number)
    file.seek(start)
    file.write(funny_bytes)
