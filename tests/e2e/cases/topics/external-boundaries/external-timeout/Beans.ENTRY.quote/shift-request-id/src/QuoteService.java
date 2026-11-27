import normalizer.intrinsics.N;

class QuoteService {
    @N.External
    private final QuoteClient client;

    QuoteService(QuoteClient client) {
        this.client = client;
    }
    int quote(int id) { return client.fetch(id + 1); }
}
class QuoteClient {
    native int fetch(int id) throws Timeout;
}
class Timeout extends RuntimeException {}

class Beans {
    static final QuoteService ENTRY = new QuoteService(new QuoteClient());
}
