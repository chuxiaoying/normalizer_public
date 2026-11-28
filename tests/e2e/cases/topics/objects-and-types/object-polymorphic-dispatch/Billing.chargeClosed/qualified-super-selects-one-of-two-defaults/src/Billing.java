interface Fee { default int apply(int value) { return value + 10; } }
interface AlternativeFee { default int apply(int value) { return value + 20; } }
class FixedFee implements Fee, AlternativeFee { public int apply(int value) { return Fee.super.apply(value) + 1; } }
class Billing {
    static int chargeClosed(int value) { Fee fee = new FixedFee(); return fee.apply(value); }
}
