interface Fee { default int apply(int value) { return value + 10; } }
interface AlternativeFee { default int apply(int value) { return value + 20; } }
class FixedFee implements Fee { public int apply(int value) { return AlternativeFee.super.apply(value); } }
class Billing {
    static int chargeClosed(int value) { Fee fee = new FixedFee(); return fee.apply(value); }
}
