pragma solidity >=0.4.0 <0.7.0;

contract DaughterContract {
  string public name;
  uint public age;
  constructor(string memory _daughtersName, uint _daughtersAge) public {
    name = _daughtersName;
    age = _daughtersAge;
  }
}

contract MomContract {
  event DoLog(address indexed _from, uint _value, uint _n);
  string public name;
  uint public age;
  DaughterContract public daughter;

  constructor(string memory _momsName, uint _momsAge, string memory _daughtersName, uint _daughtersAge) public payable {
    emit DoLog(msg.sender, msg.value, 3);
    daughter = new DaughterContract(_daughtersName, _daughtersAge);
    name = _momsName;
    age = _momsAge;
  }
}
