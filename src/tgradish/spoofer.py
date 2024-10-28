import ctypes

# must be greater than 0.0, less than 3000.0, the funnier the better
FUNNY_NUMBER = 420.69

# binary representations of the FUNNY_NUMBER according to RFC 8794, section 7.3.
FLOATS = {
    0: b'',
    4: bytes(ctypes.c_float(FUNNY_NUMBER))[::-1],
    8: bytes(ctypes.c_double(FUNNY_NUMBER))[::-1],
}

# Segment/Info/Duration path IDs of tags, converted from VINT format from here:
# https://github.com/ietf-wg-cellar/matroska-specification/blob/master/ebml_matroska.xml
ELEMENT_ID_PATH = [139690087, 88713574, 1161]

VINT_MAX_WIDTH = 4  # max amount of bytes on VINT_WIDTH and VINT_MARKER parts


def bytes2bitstr(data: bytes) -> str:
    nbytes = len(data)
    return format(int.from_bytes(data, "big"), f'0{nbytes * 8}b')


def read_vint(data: bytes) -> tuple[int, int]:
    """
    Reads Variable-Size Integer from the start of data
    according to RFC 8794 section 4.
    """
    vint_length = bytes2bitstr(data[:VINT_MAX_WIDTH]).index("1") + 1
    vint_value = int(bytes2bitstr(data[:vint_length])[vint_length:], 2)
    return vint_length, vint_value


def enter_element(s: int, f: int, data: bytes) -> tuple[int, int]:
    vint_len, element_len = read_vint(data[s:f])
    s += vint_len
    f = s + element_len
    return s, f


def skip_element(s: int, f: int, data: bytes) -> tuple[int, int]:
    vint_len, element_len = read_vint(data[s:f])
    s += vint_len + element_len
    return s, f


def find_duration_tag(data: bytes) -> tuple[int, int]:
    # start and finish of the current working byte sequence in data
    s: int = 0
    f: int = len(data)
    i: int = 0

    while s != f:
        vint_len, element_id = read_vint(data[s:f])
        s += vint_len

        if element_id == ELEMENT_ID_PATH[i]:
            s, f = enter_element(s, f, data)
            i += 1
            if i == len(ELEMENT_ID_PATH):
                return s, f
        else:
            s, f = skip_element(s, f, data)
    else:
        print("Duration value was not found")
        exit(1)


def spoof_duration(data: bytes) -> bytearray:
    s, f = find_duration_tag(data)
    data = bytearray(data)
    data[s:f] = FLOATS[f - s]
    return data


def spoof_file_duration(source_file, result_file):
    print(f"Spoofing duration of {source_file}...")

    with open(source_file, 'rb') as source:
        data = source.read()

    with open(result_file, 'wb') as result:
        result.write(spoof_duration(data))

    print("Done.")
