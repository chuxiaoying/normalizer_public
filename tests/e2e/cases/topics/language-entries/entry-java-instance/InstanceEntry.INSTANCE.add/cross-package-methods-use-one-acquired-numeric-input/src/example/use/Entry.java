package example.use;

import example.first.Tools;

public class Entry {
    public static int run(int units) {
        return Tools.change(units) - example.second.Tools.change(units);
    }
}
