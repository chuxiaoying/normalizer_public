import normalizer.intrinsics.N;

class NumberService {
    @N.External
    private final TicketIssuer issuer;

    NumberService(TicketIssuer issuer) {
        this.issuer = issuer;
    }

    int doubled() {
        int reading0 = issuer.issue();
        int reading1 = issuer.issue();
        int reading2 = issuer.issue();
        int reading3 = issuer.issue();
        int reading4 = issuer.issue();
        int reading5 = issuer.issue();
        int reading6 = issuer.issue();
        int reading7 = issuer.issue();
        int reading8 = issuer.issue();
        int reading9 = issuer.issue();
        int reading10 = issuer.issue();
        return reading10 + reading2;
    }
}

class TicketIssuer {
    native int issue();
}

class Beans {
    static final NumberService ENTRY = new NumberService(new TicketIssuer());
}
