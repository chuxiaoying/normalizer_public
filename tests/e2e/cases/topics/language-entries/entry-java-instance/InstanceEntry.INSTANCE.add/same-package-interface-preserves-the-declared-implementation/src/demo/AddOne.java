package demo;

public final class AddOne implements Op {
    public AddOne() {}

    public int apply(int x) {
        return x + 1;
    }
}
