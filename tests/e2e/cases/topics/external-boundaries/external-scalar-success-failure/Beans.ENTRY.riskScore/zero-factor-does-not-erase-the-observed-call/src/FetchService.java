import normalizer.intrinsics.N;

class FetchService {
    static final FetchService INSTANCE = new FetchService(new Client());

    @N.External
    private final Client client;

    FetchService(Client client) { this.client = client; }

    int fetch(int input, boolean enabled) {
        if (!enabled) return 1;
        return client.read(input) * 0;
    }
}

class Client { native int read(int input) throws Terminal; }
class Terminal extends RuntimeException {}
