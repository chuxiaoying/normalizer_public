interface Fee<T> { int apply(int value); }
class FixedFee implements Fee<String> { public int apply(int value) { return value - 1; } }
class RateFee implements Fee<String> { public int apply(int value) { return value / 2; } }
class ScaledFee implements Fee<String> { public int apply(int value) { return value * 2; } }
class Billing {
    static int charge(Fee<String> fee, int value) { return fee.apply(value); }

    static int chargeClosed(boolean fixed, int value) {
        Fee<String> fee = fixed ? new FixedFee() : new ScaledFee();
        return fee.apply(value);
    }
}
