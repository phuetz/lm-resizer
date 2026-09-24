require 'rspec'

RSpec.describe 'Math' do
  it 'adds numbers' do
    expect(1 + 1).to eq(3)
  end

  it 'subtracts numbers' do
    expect(2 - 1).to eq(2)
  end
end
