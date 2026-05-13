package app;

public class Calculator {
    static {
        System.out.println("warmup");
    }

    public Calculator() {
    }

    public int add(int left, int right) {
        return left + right;
    }

    public String classify(int value) {
        if (value > 10 && value < 20) {
            return "medium";
        } else if (value > 20) {
            return "large";
        }
        return "small";
    }
}
