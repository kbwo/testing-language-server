package demo

import (
	"testing"
)

func TestAddFail(t *testing.T) {
	result := Add(2, 3)
	if result != 10 {
		t.Errorf("Add(2, 3) = %d; want 5", result)
	}
}
