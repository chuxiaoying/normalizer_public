interface Parent { default int read() { return 1; } }
interface Child extends Parent {}
class Impl implements Child {}
class Billing {
    static int chargeClosed() {
        Child ref = new Impl(); return ref.read();
    }
}
