const byId = (id) => document.getElementById(id);
const input = byId("number");
const presets = document.querySelectorAll("button[data-number]");

function show(id, text) {
  byId(id).textContent = text;
}

function showRounding(step, answer) {
  const { path, binaryExponent } = step;
  if ("whole" in path) {
    show("branch", "Q16.64 · Exact integer split");
    show("branch-detail", `The binary exponent is ${binaryExponent}. The integer part is ${path.whole}. The remaining bits give the fraction exactly.`);
  } else {
    show("branch", "Q0.64 · Scale and truncate");
    const correction = path.corrected ? ", including a final × 0.1 correction" : "";
    const scaling = path.decimalShift === 0
      ? "The decimal shift is zero."
      : `Scale by 10^${path.decimalShift} in x87 precision${correction}.`;
    const shift = path.rightShift === 0
      ? `Copy ${path.significand} straight into Q0.64.`
      : `A ${path.rightShift}-bit right shift truncates ${path.significand} into Q0.64.`;
    show("branch-detail", `The binary exponent is ${binaryExponent}. ${scaling} ${shift}`);
  }
  show("fraction", step.fraction);
  show("half", `${step.half.toLocaleString("en-US")} units · ⌊2⁶³ / 10^${step.places}⌋`);
  show("sum", step.sum);
  show("carry", step.carry ? " · Carry out" : "");

  const decisions = { up: "Rounds up", keep: "Keeps the first 15 digits", down: "Rounds below the truncated value" };
  show("decision", decisions[step.decision]);
  const explanation = step.exactTie
    ? "The exact binary64 magnitude is halfway between two 15-digit decimals."
    : "The exact binary64 magnitude is off the midpoint.";
  show("decision-detail", `${explanation} Compare the first 15 digits of the exact input with the program’s result.`);
  show("truncated", `${step.truncated[0]}.${step.truncated.slice(1)}`);
  show("next", step.nextDigit);
  show("input-exponent", ` × 10^${step.inputExponent}`);
  show("rounded", `${step.rounded[0]}.${step.rounded.slice(1)} × 10^${step.roundedExponent}`);

  const scientific = answer.includes("E");
  show("shaping", scientific
    ? (Math.abs(step.roundedExponent) >= 99
      ? "The final scientific text rounds again to 14 digits and removes trailing zeros."
      : "The final scientific text uses the 15-digit result and removes trailing zeros.")
    : "The final fixed-point text places the decimal point and drops unnecessary fractional zeros.");
}

function render(inspectNumber) {
  const entered = input.value.trim();
  for (const button of presets) {
    button.setAttribute("aria-pressed", String(button.dataset.number === entered));
  }
  const json = inspectNumber(entered);
  const inspection = json === undefined ? null : JSON.parse(json);
  byId("result").hidden = !inspection;
  byId("steps").hidden = !inspection?.rounding;
  if (!inspection) {
    show("status", entered ? "Enter a finite decimal or a number in scientific notation." : "Enter a number to see its text.");
    return;
  }
  const { text, bits, rounding, subnormal } = inspection;
  show("answer", text);
  show("bits", bits);
  show("status", rounding ? "" : subnormal
    ? "A subnormal number becomes 0 before the 15-digit step."
    : "Zero of either sign becomes 0 before the 15-digit step.");
  if (rounding) showRounding(rounding, text);
}

try {
  const { default: init, inspect_number } = await import("./number-text-wasm.js");
  await init();
  const update = () => render(inspect_number);
  input.disabled = false;
  input.addEventListener("input", update);
  for (const button of presets) {
    button.disabled = false;
    button.addEventListener("click", () => {
      input.value = button.dataset.number;
      update();
    });
  }
  update();
} catch (error) {
  show("status", "The CalcFunc WebAssembly module could not start.");
  console.error(error);
}
