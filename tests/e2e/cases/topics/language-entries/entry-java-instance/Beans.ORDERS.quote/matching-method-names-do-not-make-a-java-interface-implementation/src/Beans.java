interface Pricing {
    int quote(int units);
}

class RetailPricing {
    public int quote(int units) {
        return units * 2;
    }
}

class OrderEndpoint {
    private final Pricing pricing;
    OrderEndpoint(Pricing pricing) { this.pricing = pricing; }

    int quote(int units) { return pricing.quote(units); }
}

class Beans {
    static final OrderEndpoint ORDERS = new OrderEndpoint(new RetailPricing());
}
