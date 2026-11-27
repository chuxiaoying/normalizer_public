import normalizer.intrinsics.N;

class NumberService {
    @N.External
    private final TicketIssuer issuer;

    NumberService(TicketIssuer issuer) {
        this.issuer = issuer;
    }

    int doubled() {
        int reading = issuer.issue();
        int next = issuer.issue(); return next - next;
    }
}

class TicketIssuer {
    native int issue();
}

class Beans {
    static final NumberService ENTRY = new NumberService(new TicketIssuer());
}
