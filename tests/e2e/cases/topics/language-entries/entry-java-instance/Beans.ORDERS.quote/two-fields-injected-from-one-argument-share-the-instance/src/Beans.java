interface Pricing { int quote(int units); }
class RetailPricing implements Pricing {
    public int quote(int units) { return units * 2; }
}
class OrderEndpoint {
    private final Pricing first;
    private final Pricing second;
    OrderEndpoint(Pricing first) {
        this.first = first;
        this.second = first;
    }
    int quote(int units) {
        return first == second ? first.quote(units) : -1;
    }
}
class Beans {
    static final OrderEndpoint ORDERS = new OrderEndpoint(new RetailPricing());
}
