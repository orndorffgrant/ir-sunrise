"""A simple LED + resistor circuit defined with SKiDL."""

from pathlib import Path

from skidl import (
    TEMPLATE,
    Part,
    SubCircuit,
    ground,
    netlist,
)

OUT_DIR = Path(__file__).parent / "out"


@SubCircuit
def led_indicator(vcc_net, gnd_net, resistor: str = "R", led: str = "D") -> None:
    """A current-limiting resistor in series with an LED, tied between vcc and gnd."""
    r = Part("Device", resistor, footprint="R_0805_2012Metric", dest=TEMPLATE)
    d = Part("Device", led, footprint="LED_0805_2012Metric", dest=TEMPLATE)

    r1 = r()
    d1 = d()

    r1[1] += vcc_net
    r1[2] += d1["A"]
    d1["K"] += gnd_net


def main() -> None:
    vcc = Part("power", "+3V3")
    gnd = Part("power", "GND")

    led_indicator(vcc[1], gnd[1])

    ground += gnd[1]

    OUT_DIR.mkdir(exist_ok=True)
    netlist(str(OUT_DIR / "board.net"))
    print(f"Wrote {OUT_DIR / 'board.net'}")


if __name__ == "__main__":
    main()
