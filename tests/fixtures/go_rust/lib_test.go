package go_rust

import "testing"

func Hello() string {
	return "hello"
}

func TestHello(t *testing.T) {
	if Hello() != "hello" {
		t.Fatal("expected hello")
	}
}
