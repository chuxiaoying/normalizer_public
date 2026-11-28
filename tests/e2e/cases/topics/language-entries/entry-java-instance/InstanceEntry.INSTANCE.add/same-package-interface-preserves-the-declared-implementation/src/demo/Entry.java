package demo;

public class Entry {
    public static int run(int x) {
        Op op = new AddOne();
        return op.apply(x);
    }
}
