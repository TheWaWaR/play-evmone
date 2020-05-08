pragma solidity >=0.4.0 <0.7.0;

contract CallOther {
  address simpleStorage;
  constructor(address _simpleStorage) public payable {
    simpleStorage = _simpleStorage;
  }

  function proxyGet() public view returns (uint) {
    (bool success, bytes memory data) = simpleStorage.staticcall(abi.encodeWithSignature("get()"));
    require(success);
    uint value = abi.decode(data, (uint));
    return value;
  }

  function proxySet(uint x) public payable {
    (bool success,) = simpleStorage.call(abi.encodeWithSignature("set(uint256)", x));
    require(success);
  }
}
