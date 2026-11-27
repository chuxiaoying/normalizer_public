/** @N entryState */
const quote = {display: {amount: 0, active: false}};

export function refreshQuote(units: number, expedited: boolean): void {
  const amount = props.expedited ? units + 10 : units;
  quote.display.amount = amount;
  quote.display.active = units > 0;
}
