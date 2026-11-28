interface Parent { default int read() { return 1; } }
interface Child extends Parent { int read(); }
class Impl implements Child { public int read() { return 3; } }
class Billing {
    static int chargeClosed() {
        Child ref = new Impl(); return ref.read();
    }
}
