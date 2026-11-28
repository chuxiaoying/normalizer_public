interface Parent { default int read() { return 1; } }
interface Child extends Parent { default int read() { return 2; } }
class Impl implements Child {}
class Billing {
    static int chargeClosed() {
        Parent ref = new Impl(); return ref.read();
    }
}
