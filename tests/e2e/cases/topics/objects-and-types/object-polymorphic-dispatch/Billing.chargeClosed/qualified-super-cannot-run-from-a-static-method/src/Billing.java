interface Fee { default int apply(int value) { return value + 10; } }
interface AlternativeFee { default int apply(int value) { return value + 20; } }
class FixedFee implements Fee { static int run(int value) { return Fee.super.apply(value); } }
class Billing {
    static int chargeClosed(int value) { return FixedFee.run(value); }
}
