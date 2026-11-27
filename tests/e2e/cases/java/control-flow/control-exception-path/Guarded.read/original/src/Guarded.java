class NegativeValue extends RuntimeException {}

class Guarded {
    static int read(int value) {
        try {
            if (value < 0) throw new NegativeValue();
            return value;
        } catch (NegativeValue ignored) {
            return 0;
        }
    }
}
