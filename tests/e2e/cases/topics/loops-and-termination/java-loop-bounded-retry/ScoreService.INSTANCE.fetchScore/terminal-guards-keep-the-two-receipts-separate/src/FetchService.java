import normalizer.intrinsics.N;

class FetchService {
    static final FetchService INSTANCE = new FetchService(new Client());

    @N.External
    private final Client client;

    FetchService(Client client) { this.client = client; }

    int fetch(int input) {
        try {
            return client.read(input);
        } catch (Transient ignored) {
            return client.read(input + 1);
        }
    }
}

class Client {
    native int read(int input) throws Transient, Terminal;
}
class Transient extends RuntimeException {}
class Terminal extends RuntimeException {}
