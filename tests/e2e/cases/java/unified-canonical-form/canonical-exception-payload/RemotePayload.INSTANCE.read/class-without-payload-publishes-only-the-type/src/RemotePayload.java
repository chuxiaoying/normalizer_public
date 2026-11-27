import normalizer.intrinsics.N;

class RemoteFault extends RuntimeException {  }
class Rates { native int read(int input) throws RemoteFault; }
class RemotePayload {
    static final RemotePayload INSTANCE = new RemotePayload(new Rates());

    @N.External private final Rates rates;
    RemotePayload(Rates rates) { this.rates = rates; }
    int read(int input) { return rates.read(input); }
}
