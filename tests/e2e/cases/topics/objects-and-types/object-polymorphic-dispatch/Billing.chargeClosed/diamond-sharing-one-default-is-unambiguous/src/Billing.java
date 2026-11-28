interface Parent { default int read() { return 1; } }
interface Left extends Parent {}
interface Right extends Parent {}
interface Child extends Left, Right {}
class Impl implements Child {}
class Billing {
    static int chargeClosed() {
        Child ref = new Impl(); return ref.read();
    }
}
