import ctypes, numpy as np, struct, os
lib = ctypes.CDLL(os.path.join(os.path.dirname(os.path.abspath(__file__)), "libx87sim.so"))
dp = np.ctypeslib.ndpointer(dtype=np.float64, flags="C_CONTIGUOUS")
lib.b3_batch.argtypes = [dp, dp, dp, dp, ctypes.c_int, ctypes.c_int]
B3_P = [0xC0022260ED978E7B,0xC03DE852AEE8BF62,0xC050AE34EB3CBACE,0xC0008305B479986D,0x404F0A8C053986D7,0x40409FD1AD1D8E5C,0x401572BF86842F3A,0x3FD0EE2F11884D4E,0x3F65E5B73842993C]
B3_Q = [0x3FF00000000115C7,0x4039B252B2FAAF0E,0x405EDDF2040D2BC2,0x4067B39FC919441A,0x405C515177EB165E,0x403BB43B464D3956,0x4005303658EF45CA,0x3FB52FC9914BBFB2,0x3F403CADF2CF35CF]
def f(bits): return np.array(bits, dtype=np.uint64).view(np.float64)
def b3(P, Q, xs, mode=0):
    xs = np.ascontiguousarray(xs, dtype=np.float64); out = np.empty_like(xs)
    lib.b3_batch(np.ascontiguousarray(P,dtype=np.float64), np.ascontiguousarray(Q,dtype=np.float64), xs, out, len(xs), mode)
    return out
def hx(v): return struct.pack(">d", float(v)).hex()
def window(center, half=512):
    c = np.array([center], dtype=np.float64).view(np.int64)[0]
    return (np.arange(c-half, c+half, dtype=np.int64)).view(np.float64)
def ulps(a, b):
    """signed distance a-b in ulps for same-sign positive doubles"""
    return a.view(np.int64) - b.view(np.int64)
