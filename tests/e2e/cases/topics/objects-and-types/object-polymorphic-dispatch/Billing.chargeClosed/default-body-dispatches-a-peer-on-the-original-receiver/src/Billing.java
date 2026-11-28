interface Parent { int adjust(int input); default int read(int input) { return adjust(input) + 1; } }
interface Child extends Parent {}
class Impl implements Child { public int adjust(int input) { return input * 2; } }
class Billing {
    static int chargeClosed(int value) {
        Child ref = new Impl(); return ref.read(value);
    }
}
