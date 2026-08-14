package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class(t *testing.T) {
	line2 := "Suite 42"
	home := baml_sdk.ComplexModelsPostalAddress{
		Line1:      "1 Compiler Way",
		City:       "San Francisco",
		Region:     "CA",
		PostalCode: "94107",
		Location:   &baml_sdk.ComplexModelsGeoPoint{Lat: 37.7749, Lng: -122.4194},
	}
	office := baml_sdk.ComplexModelsPostalAddress{
		Line1:      "200 Type Lane",
		Line2:      &line2,
		City:       "Oakland",
		Region:     "CA",
		PostalCode: "94612",
	}
	email := baml_sdk.ComplexModelsContactMethod{Label: "email", Value: "ada@example.com", Verified: true}
	phone := baml_sdk.ComplexModelsContactMethod{Label: "phone", Value: "+1-555-0100", Verified: false}
	notes := "first invoice"
	cardPayment := baml_sdk.NewComplexModelsCardPaymentOrComplexModelsWirePaymentFromComplexModelsCardPayment(
		baml_sdk.ComplexModelsCardPayment{Brand: "visa", Last4: "4242", BillingAddress: home},
	)
	invoice := baml_sdk.ComplexModelsInvoice{
		Id:     "inv-001",
		Status: baml_sdk.NewStringLiteral5cbcfd2eOrStringLiteralfe755c90OrStringLiteral04192b60FromStringLiteral04192b60(),
		Items: []baml_sdk.ComplexModelsLineItem{{
			Sku:        "sdk-pro",
			Quantity:   2,
			UnitPrice:  19.5,
			Tags:       []string{"sdk", "typescript"},
			Attributes: map[string]string{"language": "ts", "support": "priority"},
		}},
		Payment: &cardPayment,
		Notes:   &notes,
	}
	wirePayment := baml_sdk.NewComplexModelsCardPaymentOrComplexModelsWirePaymentFromComplexModelsWirePayment(
		baml_sdk.ComplexModelsWirePayment{
			BankName:    "Boundary Bank",
			RoutingCode: "110000000",
		},
	)
	wireInvoice := baml_sdk.ComplexModelsInvoice{
		Id:     "inv-002",
		Status: baml_sdk.NewStringLiteral5cbcfd2eOrStringLiteralfe755c90OrStringLiteral04192b60FromStringLiteralfe755c90(),
		Items: []baml_sdk.ComplexModelsLineItem{{
			Sku:        "sdk-enterprise",
			Quantity:   1,
			UnitPrice:  250,
			Tags:       []string{"sdk", "enterprise"},
			Attributes: map[string]string{"language": "python", "term": "annual"},
		}},
		Payment: &wirePayment,
	}
	featured := baml_sdk.NewStringOrComplexModelsInvoiceOrComplexModelsPostalAddressFromComplexModelsInvoice(invoice)
	want := baml_sdk.ComplexModelsComplexProfile{
		Id:   "profile-001",
		Tier: baml_sdk.ComplexModelsAccountTierEnterprise,
		Owner: baml_sdk.ComplexModelsProfileOwner{
			Name:           "Ada Lovelace",
			PrimaryContact: email,
			BackupContacts: []baml_sdk.ComplexModelsContactMethod{phone},
		},
		Addresses: []baml_sdk.ComplexModelsPostalAddress{home, office},
		Invoices:  []baml_sdk.ComplexModelsInvoice{invoice, wireInvoice},
		AuditTrail: []baml_sdk.ComplexModelsAuditEvent{
			{
				Actor:   "system",
				Action:  baml_sdk.NewStringLiterala512fd49OrStringLiterala18c08cfOrStringLiteral72bb2bacFromStringLiterala18c08cf(),
				Context: map[string]string{"source": "fixture"},
			},
			{
				Actor:   "reviewer",
				Action:  baml_sdk.NewStringLiterala512fd49OrStringLiterala18c08cfOrStringLiteral72bb2bacFromStringLiterala512fd49(),
				Context: map[string]string{"level": "2", "region": "us"},
			},
		},
		Metadata: map[string]string{"cohort": "beta", "owner_kind": "internal"},
		Featured: &featured,
		Flags: []baml_sdk.StringOrIntOrBool{
			baml_sdk.NewStringOrIntOrBoolFromInt(7),
			baml_sdk.NewStringOrIntOrBoolFromString("manual-review"),
			baml_sdk.NewStringOrIntOrBoolFromBool(true),
		},
	}

	got, err := baml_sdk.ComplexModelsRoundTripComplexProfile(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("complex profile = %#v, %v, want %#v", got, err, want)
	}
}
