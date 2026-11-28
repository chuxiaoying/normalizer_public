interface Parent { default int read() { return 1; } }
interface Child extends Parent { default int read() { return 2; } }
class Impl implements Parent, Child { public int read() { return Parent.super.read(); } }
class Billing {
    static int chargeClosed() {
        Child ref = new Impl(); return ref.read();
    }
}
