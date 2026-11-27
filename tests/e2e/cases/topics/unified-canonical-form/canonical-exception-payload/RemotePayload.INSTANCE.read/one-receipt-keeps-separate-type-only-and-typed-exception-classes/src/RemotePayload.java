import normalizer.intrinsics.N;

class AlphaFault extends RuntimeException {}
class LaterFault extends RuntimeException {}
class RemoteFault extends RuntimeException { @N.Nullable String payload; }
class ZuluFault extends RuntimeException { int payload; }
class Rates { native int read(int input) throws ZuluFault, LaterFault, RemoteFault, AlphaFault; }
class RemotePayload {
    static final RemotePayload INSTANCE = new RemotePayload(new Rates());

    @N.External private final Rates rates;
    RemotePayload(Rates rates) { this.rates = rates; }
    int read(int input) { return rates.read(input); }
}
