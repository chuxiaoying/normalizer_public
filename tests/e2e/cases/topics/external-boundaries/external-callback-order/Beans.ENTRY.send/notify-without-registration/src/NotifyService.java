import normalizer.intrinsics.N;

class NotifyService {
    @N.External
    private final Notifier notifier;

    NotifyService(Notifier notifier) {
        this.notifier = notifier;
    }
    int send(int value) {
        
        notifier.accept(value);
        return value + 1;
    }
}
class Notifier {
    native void register(int value);
    native void accept(int value);
}

class Beans {
    static final NotifyService ENTRY = new NotifyService(new Notifier());
}
