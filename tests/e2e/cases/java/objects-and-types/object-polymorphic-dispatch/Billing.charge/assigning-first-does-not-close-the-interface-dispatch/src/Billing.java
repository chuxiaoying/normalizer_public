interface Fee { int apply(int value); }
class FixedFee implements Fee { public int apply(int value) { return value - 1; } }
class RateFee implements Fee { public int apply(int value) { return value / 2; } }
class ScaledFee implements Fee { public int apply(int value) { return value * 2; } }
class Billing {
    static int charge(Fee fee, int value) { int amount = fee.apply(value); return amount; }

    static int chargeClosed(boolean fixed, int value) {
        Fee fee = fixed ? new FixedFee() : new ScaledFee();
        return fee.apply(value);
    }
}
