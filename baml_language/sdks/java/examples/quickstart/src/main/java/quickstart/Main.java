package quickstart;

public class Main {
    public static void main(String[] args) {
        long sum = baml_sdk.Fns.add(2L, 3L);
        String greeting = baml_sdk.Fns.greet("Maven");
        System.out.println("add(2, 3) = " + sum);
        System.out.println(greeting);
    }
}
