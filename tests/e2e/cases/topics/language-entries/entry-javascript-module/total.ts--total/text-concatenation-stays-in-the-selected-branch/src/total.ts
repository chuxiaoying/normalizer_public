export function total(pickup: boolean, value: string): string {
  return pickup ? "门店：" + value : "送达：" + value;
}
