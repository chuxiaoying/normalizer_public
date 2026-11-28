interface Parent { default int read() { return 1; } }
interface Other { default int read() { return 9; } }
interface Child extends Parent, Other {}
class Impl implements Child {}
class Billing {
    static int chargeClosed() {
        Child ref = new Impl(); return ref.read();
    }
}
