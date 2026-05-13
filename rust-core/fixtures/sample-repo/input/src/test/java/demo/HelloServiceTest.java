package demo;

import org.junit.jupiter.api.Test;

class HelloServiceTest {
    @Test
    void greetsByName() {
        new HelloService().greet("Sokrates");
    }
}
