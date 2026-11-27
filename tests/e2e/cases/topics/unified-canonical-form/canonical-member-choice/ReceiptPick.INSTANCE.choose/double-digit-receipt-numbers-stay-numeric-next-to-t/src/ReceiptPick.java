import normalizer.intrinsics.N;

class ReceiptPick {
    static final ReceiptPick INSTANCE = new ReceiptPick(new Checkpoints());

    @N.External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = checkpoints.read(code);
        int second = checkpoints.read(first);
        int r2 = checkpoints.read(second);
int r3 = checkpoints.read(r2);
int r4 = checkpoints.read(r3);
int r5 = checkpoints.read(r4);
int r6 = checkpoints.read(r5);
int r7 = checkpoints.read(r6);
int r8 = checkpoints.read(r7);
int r9 = checkpoints.read(r8);
int r10 = checkpoints.read(r9);
int selected;
        /* @normalizer-ir selected = choose from values else second */
        return r10 + r2 + selected;
    }
}

class Checkpoints { native int read(int code); }
