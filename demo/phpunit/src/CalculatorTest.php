<?php

namespace Tests;

use App\Calculator;
use PHPUnit\Framework\TestCase;

class CalculatorTest extends TestCase
{
    public function testAdd()
    {
        $calculator = new Calculator();
        $result = $calculator->add(2, 3);
        $this->assertEquals(5, $result);
    }

    public function testSubtract()
    {
        $calculator = new Calculator();
        $result = $calculator->subtract(5, 3);
        $this->assertEquals(2, $result);
    }

    public function testFail1()
    {
        $calculator = new Calculator();
        $result = $calculator->subtract(10, 2);
        $this->assertEquals(1, $result);
    }
}
