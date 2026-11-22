"""Read the M4 model-only witness without Excel. Uses PBIXRay 0.15.5 for the
backup stream, XML metadata and string dictionaries, and decodes the column
identifiers here from MS-XLDM sections 2.3.1.1 and 2.7.3.
"""

import json
import struct
import sys
from hashlib import sha256
from importlib.metadata import version
from itertools import repeat
from pathlib import Path
from zipfile import ZipFile

from pbixray import PBIXRay
from pbixray.utils import get_data_slice


def data_ids(data, *, records, packed_count, width, minimum):
    """Decode one non-null XMHybridRLE segment with XMRENoSplit packing."""
    if not (0 < records <= 100_000 and 0 <= packed_count <= records and 1 <= width <= 32):
        raise ValueError("segment outside the spike's supported size or packing")
    if len(data) < 16:
        raise ValueError("truncated segment")
    slots, = struct.unpack_from("<Q", data)
    subsegment = 8 + slots * 8
    if subsegment + 8 > len(data):
        raise ValueError("truncated RLE segment")
    words, = struct.unpack_from("<Q", data, subsegment)
    packed = data[subsegment + 8:]
    per_word = 64 // width
    if len(packed) != words * 8 or packed_count > words * per_word:
        raise ValueError("truncated or unsupported packed subsegment")

    result = []
    packed_at = 0
    mask = (1 << width) - 1
    for value, count in struct.iter_unpack("<iI", data[8:subsegment]):
        if len(result) == records:
            break
        if count > records - len(result):
            raise ValueError("run exceeds the metadata row count")
        if value >= 0:
            result.extend(repeat(value, count))
            continue
        if -1 - value != packed_at or packed_at + count > packed_count:
            raise ValueError("invalid packed run offset or count")
        for index in range(packed_at, packed_at + count):
            word_index, position = divmod(index, per_word)
            word, = struct.unpack_from("<Q", packed, word_index * 8)
            result.append(minimum + ((word >> (position * width)) & mask))
        packed_at += count
    if len(result) != records or packed_at != packed_count:
        raise ValueError("decoded counts disagree with column metadata")
    return result


def read_columns(model):
    source = model._metadata.source
    rows = source.schema_df
    if list(rows.TableName.unique()) != ["Table1"]:
        raise ValueError("expected the M4 witness table")
    for _, column in rows.iterrows():
        segment, = source.get_segment_meta(column, column.IDF)
        if segment["has_nulls"] or len(column.IDFs) != 1:
            raise ValueError("nulls and multiple column files are outside the spike")
        ids = data_ids(
            get_data_slice(model._data_model, column.IDF),
            records=segment["records"],
            packed_count=segment["count_bit_packed"],
            width=segment["bit_width"],
            minimum=segment["min_data_id"],
        )
        if column.DataType == "string" and isinstance(column.Dictionary, str):
            dictionary = get_data_slice(model._data_model, column.Dictionary)
            strings = model._vertipaq_decoder._read_dictionary(dictionary, column.MinDataID)
            yield column.ColumnName, [strings.values[value] for value in ids]
        elif column.DataType == "float64" and isinstance(column.HIDX, str):
            # Verified against the authored M4 inputs for the observed value encoding.
            yield column.ColumnName, [
                binary64((value + column.BaseId) / column.Magnitude) for value in ids
            ]
        else:
            raise ValueError(f"unsupported encoding for {column.ColumnName}")


def binary64(value):
    return struct.pack(">d", value).hex()


def main(workbook):
    if version("pbixray") != "0.15.5":
        raise ValueError("the spike uses the PBIXRay 0.15.5 interfaces")
    with ZipFile(workbook) as package:
        part = package.read("xl/model/item.data")
        if any(name.startswith("xl/tables/") for name in package.namelist()):
            raise ValueError("expected a workbook with the worksheet table removed")
    with PBIXRay(workbook) as model:
        columns = dict(read_columns(model))
        reference = model.get_table("Table1")
        if list(columns) != list(reference.columns):
            raise ValueError("decoded columns disagree with the reference")
        checked = reference.assign(Amount=reference.Amount.map(binary64))
        if columns != checked.to_dict(orient="list"):
            raise ValueError("decoded values disagree with the reference")
    print(json.dumps({
        "workbook_sha256": sha256(workbook.read_bytes()).hexdigest(),
        "model_sha256": sha256(part).hexdigest(),
        "worksheet_table_parts": 0,
        "rows": len(columns["Amount"]),
        "columns": columns,
    }, indent=2))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: decode_model.py m4-model-only.xlsx")
    main(Path(sys.argv[1]))
