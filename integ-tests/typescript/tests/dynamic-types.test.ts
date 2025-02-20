import TypeBuilder from '../baml_client/type_builder'
import { b } from './test-setup'

describe('Dynamic Type Tests', () => {
  describe('Basic Dynamic Types', () => {
    it('should work with dynamic types single', async () => {
      let tb = new TypeBuilder()
      tb.Person.addProperty('last_name', tb.string().optional())
      tb.Person.addProperty('height', tb.float().optional()).description('Height in meters')
      tb.Hobby.addValue('CHESS')
      tb.Hobby.listValues().map(([name, v]) => v.alias(name.toLowerCase()))
      tb.Person.addProperty('hobbies', tb.Hobby.type().list().optional()).description(
        'Some suggested hobbies they might be good at',
      )

      const res = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
        { tb },
      )
      expect(res.length).toBeGreaterThan(0)
    })

    it('should work with dynamic types enum', async () => {
      let tb = new TypeBuilder()
      const fieldEnum = tb.addEnum('Animal')
      const animals = ['giraffe', 'elephant', 'lion']
      for (const animal of animals) {
        fieldEnum.addValue(animal.toUpperCase())
      }
      tb.Person.addProperty('animalLiked', fieldEnum.type())
      const res = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop. I like giraffes.",
        { tb },
      )
      expect(res.length).toBeGreaterThan(0)
      expect(res[0]['animalLiked']).toEqual('GIRAFFE')
    })
  })

  describe('Complex Dynamic Types', () => {
    it('should work with dynamic output map', async () => {
      let tb = new TypeBuilder()
      tb.DynamicOutput.addProperty('hair_color', tb.string())
      tb.DynamicOutput.addProperty('attributes', tb.map(tb.string(), tb.string())).description(
        "Things like 'eye_color' or 'facial_hair'",
      )

      const res = await b.MyFunc(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I have blue eyes and a beard.",
        { tb },
      )

      expect(res.hair_color).toEqual('black')
      expect(res.attributes['eye_color']).toEqual('blue')
      expect(res.attributes['facial_hair']).toEqual('beard')
    })

    it('should work with dynamic output union', async () => {
      let tb = new TypeBuilder()

      const class1 = tb.addClass('Class1')
      class1.addProperty('meters', tb.float())

      const class2 = tb.addClass('Class2')
      class2.addProperty('feet', tb.float())
      class2.addProperty('inches', tb.float().optional())

      tb.DynamicOutput.addProperty('height', tb.union([class1.type(), class2.type()]))

      let res = await b.MyFunc("My name is Harrison. My hair is black and I'm 6 feet tall.", { tb })

      expect(res.height['feet']).toEqual(6)

      res = await b.MyFunc("My name is Harrison. My hair is black and I'm 1.8 meters tall.", { tb })

      expect(res.height['meters']).toEqual(1.8)
    })
  })

  describe('Add Baml', () => {
    it('should add to existing class', async () => {
      let tb = new TypeBuilder()
      tb.addBaml(`
        class ExtraPersonInfo {
            height int
            weight int
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
        }
      `)
      let res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.",
        { tb },
      )
      expect(res).toEqual([{name: "John Doe", age: 30, extra: {height: 6, weight: 180}, hair_color: "YELLOW"}])
    })

    it('should add to existing enum', async () => {
      let tb = new TypeBuilder()
      tb.addBaml(`
        dynamic enum Hobby {
          VideoGames
          BikeRiding
        }
      `)
      let res = await b.ExtractHobby("I play video games", { tb })
      expect(res).toEqual(["VideoGames"])
    })

    it('should add both class and enum', async () => {
      let tb = new TypeBuilder()
      tb.addBaml(`
        class ExtraPersonInfo {
            height int
            weight int
        }

        enum Job {
            Programmer
            Architect
            Musician
        }

        dynamic enum Hobby {
            VideoGames
            BikeRiding
        }

        dynamic enum Color {
            BROWN
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
            job Job?
            hobbies Hobby[]
        }
      `)
      let res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. My height is 6 feet and I weigh 180 pounds. My hair is brown. I work as a programmer and enjoy bike riding.",
        { tb },
      )
      expect(res).toEqual([
        {
          name: "John Doe",
          age: 30,
          hair_color: "BROWN",
          job: "Programmer",
          hobbies: ["BikeRiding"],
          extra: {height: 6, weight: 180},
        }
      ])
    })

    it('should add baml with attrs', async () => {
      let tb = new TypeBuilder()
      tb.addBaml(`
        class ExtraPersonInfo {
            height int @description("In centimeters and rounded to the nearest whole number")
            weight int @description("In kilograms and rounded to the nearest whole number")
        }

        dynamic class Person {
            extra ExtraPersonInfo?
        }
      `)
      let res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.",
        { tb },
      )
      expect(res).toEqual([{name: "John Doe", extra: {height: 183, weight: 82}, hair_color: "YELLOW"}])
    })
  })
})
