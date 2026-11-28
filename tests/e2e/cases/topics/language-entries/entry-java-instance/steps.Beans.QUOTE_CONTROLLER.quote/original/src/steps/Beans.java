package steps;


public class Beans {
    static final QuoteService QUOTE_SERVICE = new QuoteService();
    static final QuoteController QUOTE_CONTROLLER = new QuoteController();

    static {
        QUOTE_CONTROLLER.setPrice(QUOTE_SERVICE);
    }
}
