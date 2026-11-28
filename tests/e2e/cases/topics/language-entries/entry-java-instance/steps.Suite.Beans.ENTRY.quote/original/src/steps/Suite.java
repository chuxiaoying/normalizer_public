package steps;

class Suite {
    static class QuoteService {
        private int quote(int units) {
            return units * 2;
        }
    }

    static class Beans {
        static final QuoteService ENTRY = new QuoteService();
    }
}
