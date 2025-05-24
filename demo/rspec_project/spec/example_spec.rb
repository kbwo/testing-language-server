# spec/example_spec.rb
describe "MyString" do
  context "concatenation" do
    it "should concatenate two strings" do
      str1 = "Hello"
      str2 = "World"
      expect(str1 + " " + str2).to eq("Hello World")
    end
  end
end

describe "Math" do
  it "should add two numbers correctly" do
    expect(1 + 1).to eq(2)
  end

  it "should fail a test" do
    expect(1 + 1).to eq(3) # This will fail
  end

  it "is a pending test" # This is a pending test
end
