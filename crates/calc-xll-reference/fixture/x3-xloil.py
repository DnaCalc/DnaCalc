"""Small MIT fixture for the original xlOil Python plugin; no framework changes."""
import xloil


@xloil.func(name="X3.PyAdd", macro=False)
def add(a: float, b: float = 2) -> float:
    return a + b


@xloil.func(name="X3.PyHello", macro=False)
def hello(name: str) -> str:
    return "Hello, " + name


@xloil.func(name="X3.PyArray", threaded=True)
def array(n: int):
    return [[i, i * i] for i in range(n)]
