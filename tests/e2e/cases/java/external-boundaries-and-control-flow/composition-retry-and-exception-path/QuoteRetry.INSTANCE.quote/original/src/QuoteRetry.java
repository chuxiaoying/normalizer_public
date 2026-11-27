import normalizer.intrinsics.N;

class QuoteRetry {
    static final QuoteRetry INSTANCE = new QuoteRetry(new QuotePort());

    @N.External
    private final QuotePort port;

    QuoteRetry(QuotePort port) {
        this.port = port;
    }

    int quote(int accountId, int fallback) {
        for (int attempt = 0; attempt < 2; attempt++) {
            try {
                return port.quote(accountId);
            } catch (TransientQuote ignored) {
                // Only this completion permits retry.
            }
        }
        return fallback;
    }
}

class QuotePort {
    native int quote(int accountId) throws TransientQuote, TerminalQuote;
}

class TransientQuote extends RuntimeException {}
class TerminalQuote extends RuntimeException {}
