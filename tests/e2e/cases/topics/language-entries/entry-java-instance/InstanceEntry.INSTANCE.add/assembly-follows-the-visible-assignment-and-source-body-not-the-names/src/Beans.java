final class Beans {
    static final PriceService PRICE = new PriceService();
    static final QuoteController QUOTE_CONTROLLER = new QuoteController();
    static {
        QUOTE_CONTROLLER.inject(PRICE);
    }
    private Beans() {}
}
