import normalizer.intrinsics.N;

class NumberService {
    @N.External
    private final TicketIssuer issuer;

    NumberService(TicketIssuer issuer) {
        this.issuer = issuer;
    }

    int doubled() {
        int first = issuer.issue();
        int second = issuer.issue();
        return first + second;
    }
}

class TicketIssuer {
    native int issue();
}

class Beans {
    static final NumberService ENTRY = new NumberService(new TicketIssuer());
}
